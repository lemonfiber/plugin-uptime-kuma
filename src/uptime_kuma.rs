//! Uptime Kuma, asked who holds its administrator and claimed with the one lemonfiber
//! minted. A fresh Uptime Kuma holds no account, and whoever first sets it up over its
//! Socket.IO connection makes the administrator.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use lemonfiber_adapter_kit::credential::Holding;
use lemonfiber_adapter_kit::endpoint::Endpoint;
use lemonfiber_ports::http::{Http, Method, Request};
use lemonfiber_ports::service::{Failure, Owner, Ownership};
use serde_json::{json, Value};

use crate::socket::{Closed, Socket};

/// What every failure is blamed on.
pub const SERVICE: &str = "uptime-kuma";

/// How long one answer over the socket is waited for.
pub const PATIENCE: Duration = Duration::from_secs(10);

/// What Uptime Kuma answers anybody, once it has started.
const ENTRY: &str = "/api/entry-page";

const NEEDS_SETUP: &str = "needSetup";

const SETUP: &str = "setup";

const LOGIN: &str = "login";

const NO_ADMIN: &str = "this adapter holds no administrator for Uptime Kuma";

/// A client for one Uptime Kuma, claimed as the administrator `username` and `password`
/// supply.
pub struct UptimeKuma {
    endpoint: Endpoint,
    base: String,
    username: Arc<dyn Holding>,
    password: Arc<dyn Holding>,
    patience: Duration,
}

impl UptimeKuma {
    /// The Uptime Kuma reached at `base`, an `http://host:port` address, over `http` and
    /// its Socket.IO connection, claimed as the administrator `username` and `password`
    /// supply, waiting at most `patience` for each answer over the socket.
    #[must_use]
    pub fn new(
        http: Arc<dyn Http>,
        base: impl Into<String>,
        username: Arc<dyn Holding>,
        password: Arc<dyn Holding>,
        patience: Duration,
    ) -> Self {
        let base = base.into();
        Self {
            endpoint: Endpoint::new(http, base.clone(), SERVICE),
            base,
            username,
            password,
            patience,
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
        let (username, password) = self.minted()?;
        let said = socket
            .ask(
                LOGIN,
                &[json!({ "username": username, "password": password, "token": "" })],
            )
            .await
            .map_err(|_| self.unanswered())?;
        let signed_in = said.get("ok").and_then(Value::as_bool) == Some(true)
            || said.get("tokenRequired").and_then(Value::as_bool) == Some(true);
        Ok(if signed_in {
            Owner::Ours
        } else {
            Owner::Theirs
        })
    }

    /// The open connection, or nothing where Uptime Kuma does not answer yet.
    async fn connected(&self) -> Result<Option<Socket>, Failure> {
        match Socket::open(&self.base, self.patience).await {
            Ok(socket) => Ok(Some(socket)),
            Err(Closed::Unreachable) => Ok(None),
            Err(Closed::Unanswered) => Err(self.unanswered()),
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

    async fn claim(&self) -> Result<Owner, Failure> {
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
}

#[cfg(test)]
mod tests;
