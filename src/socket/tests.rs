use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use super::{Closed, Socket};

const QUICK: Duration = Duration::from_millis(500);

/// How a fake Uptime Kuma answers one connection.
#[derive(Clone)]
pub(crate) struct Script {
    /// What it says first, in place of Engine.IO's open packet.
    pub(crate) opening: String,
    /// What it answers the namespace's connect packet with.
    pub(crate) joined: Vec<String>,
    /// Each event it answers, by name, with the acknowledgement it sends; an event named
    /// more than once is answered by each in turn, the last repeating, and one it does not
    /// name goes unanswered.
    pub(crate) answers: Vec<(&'static str, String)>,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            opening: r#"0{"sid":"s","upgrades":[],"pingInterval":25000,"pingTimeout":20000}"#
                .to_owned(),
            joined: vec![
                r#"40{"sid":"n"}"#.to_owned(),
                r#"42["info",{}]"#.to_owned(),
                "2".to_owned(),
                r#"42["loginRequired"]"#.to_owned(),
            ],
            answers: Vec::new(),
        }
    }
}

/// A fake Uptime Kuma serving `script` to each connection, and every text packet it was
/// sent, across connections.
pub(crate) async fn serving(script: Script) -> (String, Arc<Mutex<Vec<String>>>) {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0").await else {
        return (String::new(), Arc::default());
    };
    let address = listener
        .local_addr()
        .map(|address| format!("http://{address}"))
        .unwrap_or_default();
    let heard: Arc<Mutex<Vec<String>>> = Arc::default();
    let kept = Arc::clone(&heard);
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let script = script.clone();
            let heard = Arc::clone(&kept);
            tokio::spawn(async move {
                let Ok(mut socket) = tokio_tungstenite::accept_async(tcp).await else {
                    return;
                };
                let mut answers = script.answers.clone();
                let _ = socket.send(Message::text(script.opening.clone())).await;
                while let Some(Ok(message)) = socket.next().await {
                    let Message::Text(text) = message else {
                        continue;
                    };
                    let text = text.as_str().to_owned();
                    if let Ok(mut heard) = heard.lock() {
                        heard.push(text.clone());
                    }
                    if text == "40" {
                        for said in &script.joined {
                            let _ = socket.send(Message::text(said.clone())).await;
                        }
                        continue;
                    }
                    let Some(rest) = text.strip_prefix("42") else {
                        continue;
                    };
                    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                    let asked: Vec<Value> = rest
                        .get(digits.len()..)
                        .and_then(|packet| serde_json::from_str(packet).ok())
                        .unwrap_or_default();
                    let event = asked.first().and_then(Value::as_str).unwrap_or_default();
                    let found: Vec<usize> = answers
                        .iter()
                        .enumerate()
                        .filter(|(_, (name, _))| *name == event)
                        .map(|(at, _)| at)
                        .collect();
                    if let Some(&at) = found.first() {
                        let answer = answers
                            .get(at)
                            .map(|(_, answer)| answer.replace("{id}", &digits));
                        if found.len() > 1 {
                            answers.remove(at);
                        }
                        if let Some(answer) = answer {
                            let _ = socket.send(Message::text(answer)).await;
                        }
                    }
                }
            });
        }
    });
    (address, heard)
}

#[tokio::test]
async fn it_joins_the_namespace_once_uptime_kuma_listens_answering_a_ping_on_the_way() {
    let (base, heard) = serving(Script::default()).await;
    let opened = Socket::open(&base, QUICK).await;
    assert!(opened.is_ok());
    tokio::time::sleep(Duration::from_millis(100)).await;
    let heard = heard.lock().map(|heard| heard.clone()).unwrap_or_default();
    assert_eq!(heard, ["40", "3"]);
}

#[tokio::test]
async fn a_packet_before_the_namespace_is_joined_is_passed_over() {
    let (base, _) = serving(Script {
        joined: vec![
            r#"42["early"]"#.to_owned(),
            r#"40{"sid":"n"}"#.to_owned(),
            r#"42["loginRequired"]"#.to_owned(),
        ],
        ..Script::default()
    })
    .await;
    assert!(Socket::open(&base, QUICK).await.is_ok());
}

#[tokio::test]
async fn an_event_is_asked_with_its_id_and_answered_by_its_acknowledgement_alone() {
    let (base, heard) = serving(Script {
        answers: vec![
            ("needSetup", r"43999[false]".to_owned()),
            ("login", r#"43{id}"unbracketed""#.to_owned()),
        ],
        ..Script::default()
    })
    .await;
    let opened = Socket::open(&base, QUICK).await;
    assert!(opened.is_ok());
    let Ok(mut socket) = opened else {
        return;
    };
    assert_eq!(socket.ask("needSetup", &[]).await, Err(Closed::Unanswered));
    assert_eq!(
        socket.ask("login", &[json!({"username": "u"})]).await,
        Err(Closed::Unanswered)
    );
    let heard = heard.lock().map(|heard| heard.clone()).unwrap_or_default();
    assert!(heard.contains(&r#"421["needSetup"]"#.to_owned()));
    assert!(heard.contains(&r#"422["login",{"username":"u"}]"#.to_owned()));
}

#[tokio::test]
async fn the_first_value_of_the_acknowledgement_is_the_answer() {
    let (base, _) = serving(Script {
        answers: vec![
            ("needSetup", r#"43{id}[true,"more"]"#.to_owned()),
            ("setup", r"43{id}[]".to_owned()),
        ],
        ..Script::default()
    })
    .await;
    let opened = Socket::open(&base, QUICK).await;
    assert!(opened.is_ok());
    let Ok(mut socket) = opened else {
        return;
    };
    assert_eq!(socket.ask("needSetup", &[]).await, Ok(json!(true)));
    assert_eq!(socket.ask("setup", &[]).await, Err(Closed::Unanswered));
}

#[tokio::test]
async fn an_address_that_is_not_plain_http_or_answers_nothing_is_unreachable() {
    for base in [
        "https://kuma:3001",
        "http://",
        "http://kuma:3001/path",
        "kuma:3001",
    ] {
        assert!(matches!(
            Socket::open(base, QUICK).await,
            Err(Closed::Unreachable)
        ));
    }
    let Ok(listener) = TcpListener::bind("127.0.0.1:0").await else {
        return;
    };
    let free = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_default();
    drop(listener);
    assert!(matches!(
        Socket::open(&format!("http://{free}"), QUICK).await,
        Err(Closed::Unreachable)
    ));
}

#[tokio::test]
async fn a_server_that_is_no_websocket_is_unreachable() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0").await else {
        return;
    };
    let base = listener
        .local_addr()
        .map(|a| format!("http://{a}"))
        .unwrap_or_default();
    tokio::spawn(async move {
        while let Ok((mut tcp, _)) = listener.accept().await {
            let _ = tokio::io::AsyncWriteExt::write_all(
                &mut tcp,
                b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n",
            )
            .await;
        }
    });
    assert!(matches!(
        Socket::open(&base, QUICK).await,
        Err(Closed::Unreachable)
    ));
}

#[tokio::test]
async fn a_server_that_never_answers_the_upgrade_is_unreachable() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0").await else {
        return;
    };
    let base = listener
        .local_addr()
        .map(|a| format!("http://{a}"))
        .unwrap_or_default();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((tcp, _)) = listener.accept().await {
            held.push(tcp);
        }
    });
    assert!(matches!(
        Socket::open(&base, Duration::from_millis(200)).await,
        Err(Closed::Unreachable)
    ));
}

#[tokio::test]
async fn what_does_not_open_join_and_listen_as_uptime_kuma_does_is_unanswered() {
    for script in [
        Script {
            opening: "not engine.io".to_owned(),
            ..Script::default()
        },
        Script {
            joined: vec![r#"44{"message":"refused"}"#.to_owned()],
            ..Script::default()
        },
        Script {
            joined: vec![r#"40{"sid":"n"}"#.to_owned()],
            ..Script::default()
        },
        Script {
            joined: Vec::new(),
            ..Script::default()
        },
        Script {
            joined: vec![r#"42["early"]"#.to_owned()],
            ..Script::default()
        },
    ] {
        let (base, _) = serving(script).await;
        assert!(matches!(
            Socket::open(&base, Duration::from_millis(300)).await,
            Err(Closed::Unanswered)
        ));
    }
}

#[tokio::test]
async fn a_connection_that_ends_before_its_answer_is_unanswered() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0").await else {
        return;
    };
    let base = listener
        .local_addr()
        .map(|a| format!("http://{a}"))
        .unwrap_or_default();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let Ok(mut socket) = tokio_tungstenite::accept_async(tcp).await else {
                continue;
            };
            let _ = socket.send(Message::binary(vec![1, 2, 3])).await;
            let _ = socket.send(Message::text("0{}")).await;
            let _ = socket.next().await;
            let _ = socket.send(Message::text("40{}")).await;
            let _ = socket.send(Message::text(r#"42["loginRequired"]"#)).await;
            let _ = socket.next().await;
            let _ = socket.close(None).await;
        }
    });
    let opened = Socket::open(&base, QUICK).await;
    assert!(opened.is_ok());
    let Ok(mut socket) = opened else {
        return;
    };
    assert_eq!(socket.ask("needSetup", &[]).await, Err(Closed::Unanswered));
    assert_eq!(socket.ask("needSetup", &[]).await, Err(Closed::Unanswered));
}
