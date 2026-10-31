//! The one Socket.IO connection Uptime Kuma's own web application opens: Engine.IO 4 over
//! a WebSocket, on which events are emitted and their acknowledgements read. Uptime Kuma
//! makes its administrator and signs one in only here, never over HTTP.
//!
//! Only what that needs is spoken: the open packet is read, the default namespace joined
//! and the event awaited that Uptime Kuma emits once it listens for the rest, a ping is
//! answered, an event is emitted with an acknowledgement id, and the acknowledgement
//! carrying that id is read back. Every other packet is passed over.

use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// Where the WebSocket transport of the default namespace is opened.
const PATH: &str = "/socket.io/?EIO=4&transport=websocket";

/// Engine.IO's open packet.
const OPEN: char = '0';

/// Engine.IO's ping, and the pong that answers it.
const PING: &str = "2";

const PONG: &str = "3";

/// Socket.IO's connect packet for the default namespace, as sent and as answered.
const CONNECT: &str = "40";

/// Socket.IO's connect error for the default namespace.
const CONNECT_ERROR: &str = "44";

/// Socket.IO's event packet, and the acknowledgement that answers one.
const EVENT: &str = "42";

/// The events Uptime Kuma emits once it has added every handler a connection is asked
/// through: before either, an event emitted is lost.
const HANDLED: [&str; 2] = [r#"42["loginRequired"]"#, r#"42["autoLogin"]"#];

const ACK: &str = "43";

/// Why a connection could not be opened, or stopped answering.
#[derive(Debug, PartialEq, Eq)]
pub enum Closed {
    /// Nothing answered at the address, or the WebSocket could not be opened.
    Unreachable,
    /// It answered, and not as Socket.IO does, or not in time.
    Unanswered,
}

/// One open Socket.IO connection to the default namespace.
pub struct Socket {
    stream: WebSocketStream<TcpStream>,
    patience: Duration,
    asked: u64,
}

impl Socket {
    /// Open the connection to the upstream at `base`, an `http://host:port` address,
    /// waiting at most `patience` for each answer.
    ///
    /// # Errors
    ///
    /// [`Closed`] where nothing answered, or what answered did not join the namespace.
    pub async fn open(base: &str, patience: Duration) -> Result<Self, Closed> {
        let authority = base
            .strip_prefix("http://")
            .map(|rest| rest.trim_end_matches('/'))
            .filter(|authority| !authority.is_empty() && !authority.contains('/'))
            .ok_or(Closed::Unreachable)?;
        let tcp = tokio::time::timeout(patience, TcpStream::connect(authority))
            .await
            .map_err(|_| Closed::Unreachable)?
            .map_err(|_| Closed::Unreachable)?;
        let (stream, _) = tokio::time::timeout(
            patience,
            tokio_tungstenite::client_async(format!("ws://{authority}{PATH}"), tcp),
        )
        .await
        .map_err(|_| Closed::Unreachable)?
        .map_err(|_| Closed::Unreachable)?;
        let mut socket = Self {
            stream,
            patience,
            asked: 0,
        };
        let opened = socket.next_text().await?;
        if !opened.starts_with(OPEN) {
            return Err(Closed::Unanswered);
        }
        socket.send(CONNECT.to_owned()).await?;
        loop {
            let said = socket.next_text().await?;
            if said.starts_with(CONNECT_ERROR) {
                return Err(Closed::Unanswered);
            }
            if said.starts_with(CONNECT) {
                break;
            }
        }
        loop {
            let said = socket.next_text().await?;
            if HANDLED.contains(&said.as_str()) {
                return Ok(socket);
            }
        }
    }

    /// Emit `event` with `arguments`, and read the first value its acknowledgement holds.
    ///
    /// # Errors
    ///
    /// [`Closed::Unanswered`] where no acknowledgement came in time, or the connection
    /// ended first.
    pub async fn ask(&mut self, event: &str, arguments: &[Value]) -> Result<Value, Closed> {
        self.asked += 1;
        let id = self.asked;
        let mut packet = vec![Value::from(event)];
        packet.extend(arguments.iter().cloned());
        self.send(format!("{EVENT}{id}{}", Value::Array(packet)))
            .await?;
        let acknowledged = format!("{ACK}{id}");
        loop {
            let said = self.next_text().await?;
            if let Some(held) = said.strip_prefix(&acknowledged) {
                if held.starts_with('[') {
                    return serde_json::from_str::<Vec<Value>>(held)
                        .ok()
                        .and_then(|values| values.into_iter().next())
                        .ok_or(Closed::Unanswered);
                }
            }
        }
    }

    async fn send(&mut self, text: String) -> Result<(), Closed> {
        tokio::time::timeout(self.patience, self.stream.send(Message::text(text)))
            .await
            .map_err(|_| Closed::Unanswered)?
            .map_err(|_| Closed::Unanswered)
    }

    /// The next text packet, a ping answered on the way.
    async fn next_text(&mut self) -> Result<String, Closed> {
        loop {
            let message = tokio::time::timeout(self.patience, self.stream.next())
                .await
                .map_err(|_| Closed::Unanswered)?
                .ok_or(Closed::Unanswered)?
                .map_err(|_| Closed::Unanswered)?;
            let Message::Text(text) = message else {
                continue;
            };
            if text.as_str() == PING {
                self.send(PONG.to_owned()).await?;
                continue;
            }
            return Ok(text.as_str().to_owned());
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
