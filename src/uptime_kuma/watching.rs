//! Uptime Kuma as the outside watcher. Its monitors are read as it emits them to its
//! administrator, and lemonfiber's own are made, updated and removed over its Socket.IO
//! connection, each holding the Uptime Kuma tag `lemonfiber` with lemonfiber's tag as its
//! value. A monitor holding no such tag is never touched. A job checks in at its push
//! monitor's address.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use lemonfiber_ports::http::{Method, Request};
use lemonfiber_ports::service::{
    Covers, Failure, Monitor, Monitoring, Provision, Provisioned, Reach,
};
use serde_json::{json, Map, Value};

use super::{UptimeKuma, SERVICE};
use crate::socket::Socket;

pub mod reading;
pub mod shaping;

const MONITOR_LIST: &str = "monitorList";

const HEARTBEAT_LIST: &str = "heartbeatList";

const ADD: &str = "add";

const EDIT: &str = "editMonitor";

const DELETE: &str = "deleteMonitor";

const GET_TAGS: &str = "getTags";

const ADD_TAG: &str = "addTag";

const ADD_MONITOR_TAG: &str = "addMonitorTag";

/// The field holding a push monitor's token.
const PUSH_TOKEN: &str = "pushToken";

/// Where a push monitor takes a check-in, before its token.
const PUSH_PATH: &str = "/api/push/";

/// What a check-in says, after the token.
const CHECKED_IN: &str = "?status=up&msg=OK";

/// The colour the Uptime Kuma tag `lemonfiber` is made in, one of Uptime Kuma's own.
const TAG_COLOUR: &str = "#D97706";

/// The bytes a push token is drawn from, written as twice as many hexadecimal digits.
const TOKEN_BYTES: usize = 16;

#[async_trait]
impl Monitoring for UptimeKuma {
    async fn reachable(&self) -> Result<Reach, Failure> {
        Ok(match self.signed_in().await {
            Ok(_) => Reach {
                reachable: true,
                why: None,
            },
            Err(unsigned) => Reach {
                reachable: false,
                why: Some(unsigned.why()),
            },
        })
    }

    async fn monitors(&self) -> Result<Vec<Monitor>, Failure> {
        let socket = self.signed_in().await.map_err(super::Unsigned::failure)?;
        let mut beats = BTreeMap::new();
        for heard in socket.heard(HEARTBEAT_LIST) {
            if let [id, Value::Array(list), ..] = heard {
                let id = id.as_str().map_or_else(|| id.to_string(), str::to_owned);
                beats.insert(id, list.clone());
            }
        }
        Ok(reading::monitors(&listed(&socket), &beats, now()))
    }

    async fn provision(&self, provision: &Provision) -> Result<Provisioned, Failure> {
        let shaped = shaping::shaped(provision).map_err(disallowed)?;
        let mut socket = self.signed_in().await.map_err(super::Unsigned::failure)?;
        let holding = holding(&listed(&socket), &provision.tag);
        let id = match holding.split_first() {
            Some((kept, more)) => {
                let id = self.edited(&mut socket, kept, shaped).await?;
                for extra in more {
                    self.deleted(&mut socket, extra).await?;
                }
                id
            }
            None => self.made(&mut socket, &provision.tag, shaped).await?,
        };
        Ok(Provisioned {
            id: id.to_string(),
            instead: None,
        })
    }

    async fn check_in(&self, tag: &str) -> Result<(), Failure> {
        ours(tag)?;
        let socket = self.signed_in().await.map_err(super::Unsigned::failure)?;
        let token = holding(&listed(&socket), tag)
            .into_iter()
            .filter(|monitor| monitor.get("type").and_then(Value::as_str) == Some(shaping::PUSH))
            .find_map(|monitor| {
                monitor
                    .get(PUSH_TOKEN)
                    .and_then(Value::as_str)
                    .filter(|token| !token.is_empty())
                    .map(str::to_owned)
            })
            .ok_or_else(|| self.endpoint.refused(&format!("no heartbeat holds {tag}")))?;
        let response = self
            .endpoint
            .send(&Request {
                method: Method::Get,
                url: self
                    .endpoint
                    .url(&format!("{PUSH_PATH}{token}{CHECKED_IN}")),
                headers: Vec::new(),
                body: None,
                pinned: None,
                deadline: Some(self.patience),
            })
            .await?;
        if response.is_success() {
            return Ok(());
        }
        let said = serde_json::from_str::<Value>(&response.body)
            .ok()
            .and_then(|said| said.get("msg").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_else(|| format!("it answered {}", response.status));
        Err(self.endpoint.refused(&format!(
            "Uptime Kuma did not take the check-in of {tag}: {said}"
        )))
    }

    async fn retire(&self, tag: &str) -> Result<(), Failure> {
        ours(tag)?;
        let mut socket = self.signed_in().await.map_err(super::Unsigned::failure)?;
        for monitor in holding(&listed(&socket), tag) {
            self.deleted(&mut socket, &monitor).await?;
        }
        Ok(())
    }
}

impl UptimeKuma {
    /// The monitor `kept` with `shaped` set on it, its push token kept where it holds one;
    /// its id.
    async fn edited(
        &self,
        socket: &mut Socket,
        kept: &Value,
        shaped: Map<String, Value>,
    ) -> Result<u64, Failure> {
        let mut monitor = kept.as_object().cloned().unwrap_or_default();
        let token = monitor
            .get(PUSH_TOKEN)
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_owned);
        monitor.extend(shaped);
        self.pushed(&mut monitor, token)?;
        let said = self.asked(socket, EDIT, &[Value::Object(monitor)]).await?;
        id_in(&said).ok_or_else(|| {
            self.endpoint
                .refused("Uptime Kuma did not say which monitor it saved")
        })
    }

    /// A monitor made with `shaped` set on it, named and tagged `tag`; its id. One that
    /// could not be tagged is removed again, so no monitor is left that nothing pairs.
    async fn made(
        &self,
        socket: &mut Socket,
        tag: &str,
        shaped: Map<String, Value>,
    ) -> Result<u64, Failure> {
        let tag_id = self.tag_id(socket).await?;
        let mut monitor = shaping::fresh(tag);
        monitor.extend(shaped);
        self.pushed(&mut monitor, None)?;
        let said = self.asked(socket, ADD, &[Value::Object(monitor)]).await?;
        let id = id_in(&said).ok_or_else(|| {
            self.endpoint
                .refused("Uptime Kuma did not say which monitor it made")
        })?;
        let tagged = self
            .asked(
                socket,
                ADD_MONITOR_TAG,
                &[Value::from(tag_id), Value::from(id), Value::from(tag)],
            )
            .await;
        if let Err(failure) = tagged {
            let _ = self
                .asked(socket, DELETE, &[Value::from(id), Value::from(false)])
                .await;
            return Err(failure);
        }
        Ok(id)
    }

    /// The monitor `monitor` removed.
    async fn deleted(&self, socket: &mut Socket, monitor: &Value) -> Result<(), Failure> {
        let id = monitor.get("id").cloned().unwrap_or(Value::Null);
        self.asked(socket, DELETE, &[id, Value::from(false)])
            .await
            .map(drop)
    }

    /// The id of the Uptime Kuma tag `lemonfiber`, made where Uptime Kuma holds none.
    async fn tag_id(&self, socket: &mut Socket) -> Result<u64, Failure> {
        let said = self.asked(socket, GET_TAGS, &[]).await?;
        let held = said
            .get("tags")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|tag| tag.get("name").and_then(Value::as_str) == Some(reading::tag_name()))
            .and_then(|tag| tag.get("id").and_then(Value::as_u64));
        if let Some(id) = held {
            return Ok(id);
        }
        let said = self
            .asked(
                socket,
                ADD_TAG,
                &[json!({ "name": reading::tag_name(), "color": TAG_COLOUR })],
            )
            .await?;
        said.get("tag")
            .and_then(|tag| tag.get("id"))
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                self.endpoint
                    .refused("Uptime Kuma did not say which tag it made")
            })
    }

    /// `monitor` holding `token`, or one freshly drawn, where it is a push monitor, and no
    /// push token where it is not, so no address takes a check-in for a check.
    fn pushed(
        &self,
        monitor: &mut Map<String, Value>,
        token: Option<String>,
    ) -> Result<(), Failure> {
        if monitor.get("type").and_then(Value::as_str) != Some(shaping::PUSH) {
            monitor.insert(PUSH_TOKEN.to_owned(), Value::Null);
            return Ok(());
        }
        let token = match token {
            Some(token) => token,
            None => self
                .random
                .bytes(TOKEN_BYTES)
                .filter(|bytes| bytes.len() == TOKEN_BYTES)
                .map(|bytes| {
                    bytes.iter().fold(String::new(), |mut token, byte| {
                        let _ = write!(token, "{byte:02x}");
                        token
                    })
                })
                .ok_or_else(|| {
                    self.endpoint
                        .refused("no unpredictable bytes could be drawn for a push token")
                })?,
        };
        monitor.insert(PUSH_TOKEN.to_owned(), Value::from(token));
        Ok(())
    }

    /// What Uptime Kuma acknowledged `event` with, where it says it did it.
    async fn asked(
        &self,
        socket: &mut Socket,
        event: &str,
        arguments: &[Value],
    ) -> Result<Value, Failure> {
        let said = socket
            .ask(event, arguments)
            .await
            .map_err(|_| self.unanswered())?;
        if said.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(said);
        }
        let reason = said
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("no reason given");
        Err(self
            .endpoint
            .refused(&format!("Uptime Kuma refused {event}: {reason}")))
    }
}

/// The list of monitors Uptime Kuma emitted last as the connection signed in, keyed by id.
fn listed(socket: &Socket) -> Value {
    socket
        .heard(MONITOR_LIST)
        .last()
        .and_then(<[Value]>::first)
        .cloned()
        .unwrap_or(Value::Null)
}

/// Every monitor in `listed` holding `tag`, by its id.
fn holding(listed: &Value, tag: &str) -> Vec<Value> {
    let mut holding: Vec<&Value> = listed
        .as_object()
        .into_iter()
        .flat_map(|listed| listed.values())
        .filter(|monitor| reading::tagged(monitor) == Some(tag))
        .collect();
    holding.sort_by_key(|monitor| monitor.get("id").and_then(Value::as_u64));
    holding.into_iter().cloned().collect()
}

/// The monitor id an acknowledgement names.
fn id_in(said: &Value) -> Option<u64> {
    said.get("monitorID").and_then(Value::as_u64)
}

/// Nothing where `tag` is one of lemonfiber's, and why it is not touched otherwise.
fn ours(tag: &str) -> Result<(), Failure> {
    match Covers::read(tag) {
        Some(_) => Ok(()),
        None => Err(disallowed(format!("{tag} is not a tag of lemonfiber's"))),
    }
}

/// A provision or a tag refused for `why`, which asking again is refused for too.
fn disallowed(why: String) -> Failure {
    Failure::Disallowed {
        service: SERVICE.to_owned(),
        detail: why,
    }
}

/// Now, in seconds since the Unix epoch.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests;
