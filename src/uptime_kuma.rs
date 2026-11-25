//! Uptime Kuma, asked who holds its administrator and claimed with the one lemonfiber
//! minted. A fresh Uptime Kuma holds no account, and whoever first sets it up over its
//! Socket.IO connection makes the administrator.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use lemonfiber_adapter_kit::credential::Holding;
use lemonfiber_adapter_kit::endpoint::Endpoint;
use lemonfiber_ports::http::{Http, Method, Request};
use lemonfiber_ports::random::Random;
use lemonfiber_ports::service::{Claim, Failure, Owner, Ownership};
use serde_json::{json, Value};

use crate::socket::{Closed, Socket};

pub mod watching;

/// What every failure is blamed on.
pub const SERVICE: &str = "uptime-kuma";

/// How long one answer over the socket is waited for.
pub const PATIENCE: Duration = Duration::from_secs(10);

/// What Uptime Kuma answers anybody, once it has started.
const ENTRY: &str = "/api/entry-page";

const NEEDS_SETUP: &str = "needSetup";

const SETUP: &str = "setup";

const LOGIN: &str = "login";

/// Signs in with the token an earlier sign-in answered, which Uptime Kuma does not count
/// against the twenty sign-ins a minute it takes with a password.
const LOGIN_BY_TOKEN: &str = "loginByToken";

const NO_ADMIN: &str = "this adapter holds no administrator for Uptime Kuma";

const CLAIMED_WITH_OWN_SECRET: &str =
    "Uptime Kuma is claimed with the administrator lemonfiber minted, which no outside service names";

/// What Uptime Kuma answers a name and password it holds no account for with.
const INCORRECT: &str = "authIncorrectCreds";

/// What signing in with the minted name and password came to.
enum SignIn {
    /// Signed in.
    In,
    /// The account asks for a second factor.
    SecondFactor,
    /// Uptime Kuma holds no such account.
    Wrong,
}

/// Why the connection could not be signed in as the minted administrator.
enum Unsigned {
    /// Uptime Kuma does not answer yet.
    Settling,
    /// Uptime Kuma waits to be set up.
    Waiting,
    /// The minted administrator does not sign in.
    Theirs,
    /// The minted administrator asks for a second factor.
    SecondFactor,
    /// Asking failed.
    Failed(Failure),
}

impl Unsigned {
    /// Why, in words.
    fn why(&self) -> String {
        match self {
            Self::Settling => "Uptime Kuma is not answering yet".to_owned(),
            Self::Waiting => "Uptime Kuma waits to be set up".to_owned(),
            Self::Theirs => {
                "the administrator lemonfiber minted does not sign in to Uptime Kuma".to_owned()
            }
            Self::SecondFactor => {
                "the administrator lemonfiber minted asks Uptime Kuma for a second factor"
                    .to_owned()
            }
            Self::Failed(failure) => failure.to_string(),
        }
    }

    /// The failure an operation that needs the signed-in connection answers.
    fn failure(self) -> Failure {
        match self {
            Self::Settling | Self::Waiting => Failure::Unavailable {
                service: SERVICE.to_owned(),
            },
            Self::Theirs | Self::SecondFactor => Failure::Unsupported {
                service: SERVICE.to_owned(),
                detail: format!("{}, so its monitors cannot be configured", self.why()),
            },
            Self::Failed(failure) => failure,
        }
    }
}

/// A client for one Uptime Kuma, claimed as the administrator `username` and `password`
/// supply.
pub struct UptimeKuma {
    endpoint: Endpoint,
    base: String,
    username: Arc<dyn Holding>,
    password: Arc<dyn Holding>,
    patience: Duration,
    random: Arc<dyn Random>,
    token: Mutex<Option<String>>,
}

impl UptimeKuma {
    /// The Uptime Kuma reached at `base`, an `http://host:port` address, over `http` and
    /// its Socket.IO connection, claimed as the administrator `username` and `password`
    /// supply, waiting at most `patience` for each answer over the socket, and drawing
    /// each heartbeat's push token from `random`.
    #[must_use]
    pub fn new(
        http: Arc<dyn Http>,
        base: impl Into<String>,
        username: Arc<dyn Holding>,
        password: Arc<dyn Holding>,
        patience: Duration,
        random: Arc<dyn Random>,
    ) -> Self {
        let base = base.into();
        Self {
            endpoint: Endpoint::new(http, base.clone(), SERVICE),
            base,
            username,
            password,
            patience,
            random,
            token: Mutex::new(None),
        }
    }

    /// Whether Uptime Kuma answers its entry page, which it answers to anybody.
    pub async fn answers(&self) -> bool {
        self.endpoint
            .send(&Request {
                method: Method::Get,
                url: self.endpoint.url(ENTRY),
                headers: Vec::new(),
                body: None,
                pinned: None,
                deadline: Some(self.patience),
            })
            .await
            .is_ok_and(|response| response.is_success())
    }

    fn minted(&self) -> Result<(String, String), Failure> {
        let held = |holding: &Arc<dyn Holding>| {
            holding
                .key()
                .map_err(|why| self.endpoint.refused(&format!("{NO_ADMIN}: {why}")))
        };
        Ok((held(&self.username)?, held(&self.password)?))
    }

    /// What answered at Uptime Kuma's address did not answer over its socket as it does.
    fn unanswered(&self) -> Failure {
        self.endpoint
            .refused("Uptime Kuma did not answer over its Socket.IO connection as it does")
    }

    /// Whether Uptime Kuma still waits to be set up.
    async fn needs_setup(&self, socket: &mut Socket) -> Result<bool, Failure> {
        let said = socket
            .ask(NEEDS_SETUP, &[])
            .await
            .map_err(|_| self.unanswered())?;
        said.as_bool().ok_or_else(|| {
            self.endpoint
                .refused("Uptime Kuma did not say whether it waits to be set up")
        })
    }

    /// `Ours` where the minted administrator signs in, and `Theirs` otherwise. An account
    /// that asks for a second factor signed in with the minted password is ours: only its
    /// holder could have turned that on.
    async fn held_by(&self, socket: &mut Socket) -> Result<Owner, Failure> {
        match self.sign_in(socket).await? {
            SignIn::In | SignIn::SecondFactor => Ok(Owner::Ours),
            SignIn::Wrong => Ok(Owner::Theirs),
        }
    }

    /// What signing in as the minted administrator came to, the token it answered kept for
    /// the next connection.
    async fn sign_in(&self, socket: &mut Socket) -> Result<SignIn, Failure> {
        let (username, password) = self.minted()?;
        let said = socket
            .ask(
                LOGIN,
                &[json!({ "username": username, "password": password, "token": "" })],
            )
            .await
            .map_err(|_| self.unanswered())?;
        if said.get("ok").and_then(Value::as_bool) == Some(true) {
            self.keep_token(said.get("token").and_then(Value::as_str));
            return Ok(SignIn::In);
        }
        if said.get("tokenRequired").and_then(Value::as_bool) == Some(true) {
            return Ok(SignIn::SecondFactor);
        }
        match said.get("msg").and_then(Value::as_str) {
            None | Some(INCORRECT) => Ok(SignIn::Wrong),
            Some(reason) => Err(self.endpoint.refused(&format!(
                "Uptime Kuma would not sign in the administrator lemonfiber minted: {reason}"
            ))),
        }
    }

    /// Who holds Uptime Kuma once it is claimed for the minted administrator where it
    /// waits to be set up.
    async fn claimed(&self) -> Result<Owner, Failure> {
        let Some(mut socket) = self.connected().await? else {
            return Ok(Owner::Settling);
        };
        if !self.needs_setup(&mut socket).await? {
            return self.held_by(&mut socket).await;
        }
        let (username, password) = self.minted()?;
        let said = socket
            .ask(SETUP, &[Value::from(username), Value::from(password)])
            .await
            .map_err(|_| self.unanswered())?;
        if said.get("ok").and_then(Value::as_bool) != Some(true) {
            if self.needs_setup(&mut socket).await? {
                let reason = said
                    .get("msg")
                    .and_then(Value::as_str)
                    .unwrap_or("no reason given");
                return Err(self.endpoint.refused(&format!(
                    "Uptime Kuma would not take the administrator lemonfiber minted: {reason}"
                )));
            }
            return self.held_by(&mut socket).await;
        }
        match self.held_by(&mut socket).await? {
            Owner::Ours => Ok(Owner::Ours),
            _ => Err(self.endpoint.refused(
                "Uptime Kuma took the administrator lemonfiber minted and does not sign it in",
            )),
        }
    }

    /// The open connection, or nothing where Uptime Kuma does not answer yet.
    async fn connected(&self) -> Result<Option<Socket>, Failure> {
        match Socket::open(&self.base, self.patience).await {
            Ok(socket) => Ok(Some(socket)),
            Err(Closed::Unreachable) => Ok(None),
            Err(Closed::Unanswered) => Err(self.unanswered()),
        }
    }

    /// The connection signed in as the minted administrator, holding what Uptime Kuma
    /// emitted as it signed in: by the token an earlier sign-in answered where it still
    /// holds, and by the minted name and password otherwise.
    async fn signed_in(&self) -> Result<Socket, Unsigned> {
        let mut socket = match self.connected().await {
            Ok(Some(socket)) => socket,
            Ok(None) => return Err(Unsigned::Settling),
            Err(failure) => return Err(Unsigned::Failed(failure)),
        };
        let held = self.token.lock().ok().and_then(|token| token.clone());
        if let Some(token) = held {
            let said = socket
                .ask(LOGIN_BY_TOKEN, &[Value::from(token)])
                .await
                .map_err(|_| Unsigned::Failed(self.unanswered()))?;
            if said.get("ok").and_then(Value::as_bool) == Some(true) {
                return Ok(socket);
            }
            self.keep_token(None);
        }
        if self
            .needs_setup(&mut socket)
            .await
            .map_err(Unsigned::Failed)?
        {
            return Err(Unsigned::Waiting);
        }
        match self.sign_in(&mut socket).await.map_err(Unsigned::Failed)? {
            SignIn::In => Ok(socket),
            SignIn::SecondFactor => Err(Unsigned::SecondFactor),
            SignIn::Wrong => Err(Unsigned::Theirs),
        }
    }

    fn keep_token(&self, token: Option<&str>) {
        if let Ok(mut held) = self.token.lock() {
            *held = token.map(str::to_owned);
        }
    }
}

#[async_trait]
impl Ownership for UptimeKuma {
    async fn owner(&self) -> Result<Owner, Failure> {
        let Some(mut socket) = self.connected().await? else {
            return Ok(Owner::Settling);
        };
        if self.needs_setup(&mut socket).await? {
            return Ok(Owner::Open);
        }
        self.held_by(&mut socket).await
    }

    async fn claim(&self) -> Result<Claim, Failure> {
        self.claimed().await.map(Claim::Held)
    }

    async fn claimed_by(&self) -> Result<Option<String>, Failure> {
        Err(Failure::Unsupported {
            service: SERVICE.to_owned(),
            detail: CLAIMED_WITH_OWN_SECRET.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests;
