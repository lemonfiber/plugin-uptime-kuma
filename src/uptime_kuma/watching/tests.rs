use std::sync::{Arc, Mutex};
use std::time::Duration;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_fixtures::ports::Chance;
use lemonfiber_ports::http::Http;
use lemonfiber_ports::service::{
    Failure, MonitorKind, MonitorState, Monitoring as _, Provision, Provisioned, Reach, Statuses,
};
use serde_json::{json, Value};

use crate::socket::tests::{serving, Script};
use crate::uptime_kuma::UptimeKuma;

const QUICK: Duration = Duration::from_millis(500);

const TAG: &str = "lemonfiber:job:guard";

/// The push token drawn from the cycling bytes `a` to `p`.
const DRAWN: &str = "6162636465666768696a6b6c6d6e6f70";

const SET_UP: (&str, &str) = ("needSetup", r"43{id}[false]");

const WAITING: (&str, &str) = ("needSetup", r"43{id}[true]");

fn kuma(base: &str, http: Arc<dyn Http>) -> UptimeKuma {
    UptimeKuma::new(
        http,
        base,
        Arc::new("admin".to_owned()),
        Arc::new("Ab1-password".to_owned()),
        QUICK,
        Arc::new(Chance::cycling()),
    )
}

/// A sign-in answered with `listed` and each monitor's `beats` emitted before it.
fn signed_in(listed: &Value, beats: &[(Value, Value)]) -> String {
    let mut packets = vec![format!(r#"42["monitorList",{listed}]"#)];
    packets.extend(
        beats
            .iter()
            .map(|(id, list)| format!(r#"42["heartbeatList",{id},{list},false]"#)),
    );
    packets.push(r#"43{id}[{"ok":true,"token":"jwt"}]"#.to_owned());
    packets.join("\n")
}

fn script(answers: Vec<(&'static str, String)>) -> Script {
    Script {
        answers,
        ..Script::default()
    }
}

fn answered(listed: &Value, more: Vec<(&'static str, &str)>) -> Script {
    let mut answers = vec![
        (SET_UP.0, SET_UP.1.to_owned()),
        ("login", signed_in(listed, &[])),
    ];
    answers.extend(
        more.into_iter()
            .map(|(event, said)| (event, said.to_owned())),
    );
    script(answers)
}

fn heard(packets: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    packets
        .lock()
        .map(|heard| heard.clone())
        .unwrap_or_default()
}

fn asked_with(packets: &Arc<Mutex<Vec<String>>>, event: &str) -> Vec<Value> {
    heard(packets)
        .iter()
        .filter_map(|packet| {
            let rest = packet.strip_prefix("42")?;
            let at = rest.find('[')?;
            serde_json::from_str::<Vec<Value>>(rest.get(at..)?).ok()
        })
        .filter(|asked| asked.first().and_then(Value::as_str) == Some(event))
        .map(|asked| Value::Array(asked.into_iter().skip(1).collect()))
        .collect()
}

fn ours(id: u64, tag: &str, kind: &str) -> Value {
    json!({
        "id": id, "name": format!("watch {id}"), "type": kind, "active": true,
        "pushToken": "kept-token", "interval": 60, "retryInterval": 60,
        "tags": [{ "tag_id": 1, "monitor_id": id, "name": "lemonfiber", "value": tag, "color": "#D97706" }],
    })
}

fn heartbeat() -> Provision {
    Provision {
        tag: TAG.to_owned(),
        kind: MonitorKind::Heartbeat,
        target: "guard".to_owned(),
        every_s: 60,
        grace_s: Some(60),
        retries: 0,
        accepted: None,
    }
}

fn detail(outcome: Result<impl std::fmt::Debug, Failure>) -> String {
    match outcome {
        Err(
            Failure::Refused { detail, .. }
            | Failure::Unsupported { detail, .. }
            | Failure::Disallowed { detail, .. },
        ) => detail,
        other => format!("{other:?}"),
    }
}

#[tokio::test]
async fn it_is_reachable_where_the_minted_administrator_signs_in_and_says_why_not_otherwise() {
    let (base, _) = serving(answered(&json!({}), Vec::new())).await;
    assert_eq!(
        kuma(&base, Fake::silent()).reachable().await.ok(),
        Some(Reach {
            reachable: true,
            why: None
        })
    );
    let why = |answers: Vec<(&'static str, &'static str)>| async move {
        let answers = answers
            .into_iter()
            .map(|(event, said)| (event, said.to_owned()))
            .collect();
        let (base, _) = serving(script(answers)).await;
        kuma(&base, Fake::silent())
            .reachable()
            .await
            .ok()
            .and_then(|reach| reach.why)
            .unwrap_or_default()
    };
    assert_eq!(why(vec![WAITING]).await, "Uptime Kuma waits to be set up");
    assert_eq!(
        why(vec![
            SET_UP,
            (
                "login",
                r#"43{id}[{"ok":false,"msg":"authIncorrectCreds"}]"#
            )
        ])
        .await,
        "the administrator lemonfiber minted does not sign in to Uptime Kuma"
    );
    assert_eq!(
        why(vec![SET_UP, ("login", r#"43{id}[{"ok":false}]"#)]).await,
        "the administrator lemonfiber minted does not sign in to Uptime Kuma"
    );
    assert_eq!(
        why(vec![SET_UP, ("login", r#"43{id}[{"tokenRequired":true}]"#)]).await,
        "the administrator lemonfiber minted asks Uptime Kuma for a second factor"
    );
    assert!(why(vec![
        SET_UP,
        (
            "login",
            r#"43{id}[{"ok":false,"msg":"Too frequently, try again later."}]"#
        )
    ])
    .await
    .contains("would not sign in the administrator lemonfiber minted: Too frequently"));
    assert!(why(vec![SET_UP])
        .await
        .contains("did not answer over its Socket.IO connection"));
    let (opened_wrong, _) = serving(Script {
        opening: "nope".to_owned(),
        ..Script::default()
    })
    .await;
    assert!(kuma(&opened_wrong, Fake::silent())
        .reachable()
        .await
        .ok()
        .and_then(|reach| reach.why)
        .is_some_and(|why| why.contains("did not answer over its Socket.IO connection")));
    let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await else {
        return;
    };
    let free = listener
        .local_addr()
        .map(|address| format!("http://{address}"))
        .unwrap_or_default();
    drop(listener);
    assert_eq!(
        kuma(&free, Fake::silent())
            .reachable()
            .await
            .ok()
            .and_then(|reach| reach.why),
        Some("Uptime Kuma is not answering yet".to_owned())
    );
}

/// Which sign-in each connection `packets` heard made.
fn sign_ins(packets: &Arc<Mutex<Vec<String>>>) -> Vec<&'static str> {
    heard(packets)
        .iter()
        .filter_map(|packet| {
            ["login", "loginByToken"]
                .into_iter()
                .find(|event| packet.contains(&format!("[\"{event}\"")))
        })
        .collect()
}

#[tokio::test]
async fn a_second_connection_signs_in_by_the_token_and_by_the_password_once_it_no_longer_holds() {
    let (holding, held) = serving(script(vec![
        (SET_UP.0, SET_UP.1.to_owned()),
        ("login", signed_in(&json!({}), &[])),
        ("loginByToken", signed_in(&json!({}), &[])),
    ]))
    .await;
    let kuma_holding = kuma(&holding, Fake::silent());
    for _ in 0..2 {
        assert!(kuma_holding
            .reachable()
            .await
            .is_ok_and(|reach| reach.reachable));
    }
    assert_eq!(sign_ins(&held), ["login", "loginByToken"]);
    assert_eq!(asked_with(&held, "loginByToken"), [json!(["jwt"])]);
    let (lapsed, packets) = serving(script(vec![
        (SET_UP.0, SET_UP.1.to_owned()),
        ("login", signed_in(&json!({}), &[])),
        (
            "loginByToken",
            r#"43{id}[{"ok":false,"msg":"authInvalidToken"}]"#.to_owned(),
        ),
    ]))
    .await;
    let kuma_lapsed = kuma(&lapsed, Fake::silent());
    for _ in 0..2 {
        assert!(kuma_lapsed
            .reachable()
            .await
            .is_ok_and(|reach| reach.reachable));
    }
    assert_eq!(sign_ins(&packets), ["login", "loginByToken", "login"]);
}

#[tokio::test]
async fn a_token_sign_in_nothing_answers_reads_unreachable_and_says_why() {
    let (base, packets) = serving(script(vec![
        (SET_UP.0, SET_UP.1.to_owned()),
        ("login", signed_in(&json!({}), &[])),
    ]))
    .await;
    let kuma = kuma(&base, Fake::silent());
    assert!(kuma.reachable().await.is_ok_and(|reach| reach.reachable));
    assert!(kuma
        .reachable()
        .await
        .is_ok_and(|reach| !reach.reachable && reach.why.is_some()));
    assert_eq!(sign_ins(&packets), ["login", "loginByToken"]);
}

#[tokio::test]
async fn every_monitor_is_read_with_its_beats_and_a_monitor_nobody_signed_in_to_is_unsupported() {
    let listed = json!({ "1": ours(1, "lemonfiber:service:sonarr", "http"), "2": { "id": 2, "type": "group" } });
    let (base, _) = serving(script(vec![
        (SET_UP.0, SET_UP.1.to_owned()),
        (
            "login",
            signed_in(
                &listed,
                &[
                    (
                        json!(1),
                        json!([{ "status": 0, "time": "2026-11-08 12:00:00", "important": true }]),
                    ),
                    (json!("2"), json!("not a list")),
                ],
            ),
        ),
    ]))
    .await;
    let read = kuma(&base, Fake::silent()).monitors().await;
    assert_eq!(
        read.ok().map(|read| read
            .into_iter()
            .map(|monitor| (monitor.id, monitor.tag, monitor.state, monitor.since))
            .collect::<Vec<_>>()),
        Some(vec![(
            "1".to_owned(),
            Some("lemonfiber:service:sonarr".to_owned()),
            MonitorState::Down,
            1_794_139_200
        )])
    );
    let (theirs, _) = serving(script(vec![
        (SET_UP.0, SET_UP.1.to_owned()),
        (
            "login",
            r#"43{id}[{"ok":false,"msg":"authIncorrectCreds"}]"#.to_owned(),
        ),
    ]))
    .await;
    assert_eq!(
        detail(kuma(&theirs, Fake::silent()).monitors().await),
        "the administrator lemonfiber minted does not sign in to Uptime Kuma, so its monitors \
         cannot be configured"
    );
    let (waiting, _) = serving(script(vec![(WAITING.0, WAITING.1.to_owned())])).await;
    assert!(matches!(
        kuma(&waiting, Fake::silent()).monitors().await,
        Err(Failure::Unavailable { .. })
    ));
    let (unanswered, _) = serving(script(vec![(SET_UP.0, SET_UP.1.to_owned())])).await;
    assert!(detail(kuma(&unanswered, Fake::silent()).monitors().await)
        .contains("did not answer over its Socket.IO connection"));
}

#[tokio::test]
async fn a_tag_nothing_holds_is_made_a_monitor_named_and_tagged_by_it_under_a_fresh_tag() {
    let (base, packets) = serving(answered(
        &json!({ "3": ours(3, "lemonfiber:job:other", "push") }),
        vec![
            (
                "getTags",
                r#"43{id}[{"ok":true,"tags":[{"id":1,"name":"other"}]}]"#,
            ),
            (
                "addTag",
                r#"43{id}[{"ok":true,"tag":{"id":5,"name":"lemonfiber"}}]"#,
            ),
            ("add", r#"43{id}[{"ok":true,"monitorID":7}]"#),
            ("addMonitorTag", r#"43{id}[{"ok":true}]"#),
        ],
    ))
    .await;
    assert_eq!(
        kuma(&base, Fake::silent())
            .provision(&heartbeat())
            .await
            .ok(),
        Some(Provisioned {
            id: "7".to_owned(),
            instead: None
        })
    );
    assert_eq!(
        asked_with(&packets, "addTag"),
        [json!([{ "name": "lemonfiber", "color": "#D97706" }])]
    );
    let made = asked_with(&packets, "add");
    let made = made
        .first()
        .and_then(|asked| asked.get(0))
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        (
            made.get("name"),
            made.get("type"),
            made.get("pushToken"),
            made.get("interval"),
            made.get("retryInterval"),
            made.get("maxretries"),
        ),
        (
            Some(&json!(TAG)),
            Some(&json!("push")),
            Some(&json!(DRAWN)),
            Some(&json!(120)),
            Some(&json!(60)),
            Some(&json!(0)),
        )
    );
    assert_eq!(asked_with(&packets, "addMonitorTag"), [json!([5, 7, TAG])]);
}

#[tokio::test]
async fn a_tag_one_monitor_holds_updates_it_in_place_and_removes_any_other_holding_it() {
    let listed = json!({
        "4": ours(4, TAG, "push"),
        "9": ours(9, TAG, "push"),
        "5": ours(5, "lemonfiber:job:other", "push"),
        "6": { "id": 6, "name": "mine", "type": "push", "pushToken": "", "tags": [] },
    });
    let (base, packets) = serving(answered(
        &listed,
        vec![
            ("editMonitor", r#"43{id}[{"ok":true,"monitorID":4}]"#),
            ("deleteMonitor", r#"43{id}[{"ok":true}]"#),
        ],
    ))
    .await;
    assert_eq!(
        kuma(&base, Fake::silent())
            .provision(&heartbeat())
            .await
            .ok()
            .map(|made| made.id),
        Some("4".to_owned())
    );
    let edited = asked_with(&packets, "editMonitor");
    let edited = edited
        .first()
        .and_then(|asked| asked.get(0))
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        (
            edited.get("id"),
            edited.get("name"),
            edited.get("pushToken"),
            edited.get("interval")
        ),
        (
            Some(&json!(4)),
            Some(&json!("watch 4")),
            Some(&json!("kept-token")),
            Some(&json!(120))
        )
    );
    assert_eq!(asked_with(&packets, "deleteMonitor"), [json!([9, false])]);
    assert!(asked_with(&packets, "add").is_empty());
}

#[tokio::test]
async fn a_check_lemonfiber_holds_reads_up_on_the_statuses_told_is_left_no_push_token_and_is_not_made_again(
) {
    let held = ours(4, "lemonfiber:service:sonarr", "http");
    let (base, packets) = serving(answered(
        &json!({ "4": held }),
        vec![("editMonitor", r#"43{id}[{"ok":true,"monitorID":4}]"#)],
    ))
    .await;
    let check = Provision {
        tag: "lemonfiber:service:sonarr".to_owned(),
        kind: MonitorKind::Http,
        target: "http://sonarr:8989/ping".to_owned(),
        every_s: 60,
        grace_s: None,
        retries: 3,
        accepted: Some(vec![Statuses { from: 200, to: 399 }]),
    };
    assert!(kuma(&base, Fake::silent()).provision(&check).await.is_ok());
    let edited = asked_with(&packets, "editMonitor");
    let edited = edited
        .first()
        .and_then(|asked| asked.get(0))
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        (
            edited.get("url"),
            edited.get("maxretries"),
            edited.get("pushToken"),
            edited.get("accepted_statuscodes")
        ),
        (
            Some(&json!("http://sonarr:8989/ping")),
            Some(&json!(2)),
            Some(&Value::Null),
            Some(&json!(["200-399"]))
        )
    );
    let (fresh, fresh_packets) = serving(answered(
        &json!({}),
        vec![
            (
                "getTags",
                r#"43{id}[{"ok":true,"tags":[{"id":2,"name":"lemonfiber"}]}]"#,
            ),
            ("add", r#"43{id}[{"ok":true,"monitorID":8}]"#),
            ("addMonitorTag", r#"43{id}[{"ok":true}]"#),
        ],
    ))
    .await;
    assert!(kuma(&fresh, Fake::silent()).provision(&check).await.is_ok());
    assert!(asked_with(&fresh_packets, "addTag").is_empty());
    assert_eq!(
        asked_with(&fresh_packets, "addMonitorTag"),
        [json!([2, 8, "lemonfiber:service:sonarr"])]
    );
}

#[tokio::test]
async fn a_held_heartbeat_whose_token_is_empty_is_given_one() {
    let mut held = ours(4, TAG, "push");
    if let Some(fields) = held.as_object_mut() {
        fields.insert("pushToken".to_owned(), json!(""));
    }
    let (base, packets) = serving(answered(
        &json!({ "4": held }),
        vec![("editMonitor", r#"43{id}[{"ok":true,"monitorID":4}]"#)],
    ))
    .await;
    assert!(kuma(&base, Fake::silent())
        .provision(&heartbeat())
        .await
        .is_ok());
    let edited = asked_with(&packets, "editMonitor");
    assert_eq!(
        edited
            .first()
            .and_then(|asked| asked.get(0))
            .and_then(|monitor| monitor.get("pushToken")),
        Some(&json!(DRAWN))
    );
}

#[tokio::test]
async fn a_monitor_that_could_not_be_tagged_is_removed_again() {
    let (base, packets) = serving(answered(
        &json!({}),
        vec![
            (
                "getTags",
                r#"43{id}[{"ok":true,"tags":[{"id":2,"name":"lemonfiber"}]}]"#,
            ),
            ("add", r#"43{id}[{"ok":true,"monitorID":8}]"#),
            (
                "addMonitorTag",
                r#"43{id}[{"ok":false,"msg":"SQLITE_BUSY"}]"#,
            ),
            ("deleteMonitor", r#"43{id}[{"ok":true}]"#),
        ],
    ))
    .await;
    assert_eq!(
        detail(kuma(&base, Fake::silent()).provision(&heartbeat()).await),
        "Uptime Kuma refused addMonitorTag: SQLITE_BUSY"
    );
    assert_eq!(asked_with(&packets, "deleteMonitor"), [json!([8, false])]);
}

#[tokio::test]
async fn what_uptime_kuma_refuses_or_does_not_name_is_refused() {
    let tags = ("getTags", r#"43{id}[{"ok":true,"tags":[]}]"#);
    for (answers, said) in [
        (
            vec![tags, ("addTag", r#"43{id}[{"ok":true}]"#)],
            "Uptime Kuma did not say which tag it made",
        ),
        (
            vec![
                (
                    "getTags",
                    r#"43{id}[{"ok":true,"tags":[{"id":2,"name":"lemonfiber"}]}]"#,
                ),
                (
                    "add",
                    r#"43{id}[{"ok":false,"msg":"Interval cannot be less than 20 seconds"}]"#,
                ),
            ],
            "Uptime Kuma refused add: Interval cannot be less than 20 seconds",
        ),
        (
            vec![
                (
                    "getTags",
                    r#"43{id}[{"ok":true,"tags":[{"id":2,"name":"lemonfiber"}]}]"#,
                ),
                ("add", r#"43{id}[{"ok":true}]"#),
            ],
            "Uptime Kuma did not say which monitor it made",
        ),
        (
            vec![("getTags", r#"43{id}[{"ok":false}]"#)],
            "Uptime Kuma refused getTags: no reason given",
        ),
    ] {
        let (base, _) = serving(answered(&json!({}), answers)).await;
        assert_eq!(
            detail(kuma(&base, Fake::silent()).provision(&heartbeat()).await),
            said
        );
    }
    let (edited, _) = serving(answered(
        &json!({ "4": ours(4, TAG, "push") }),
        vec![("editMonitor", r#"43{id}[{"ok":true}]"#)],
    ))
    .await;
    assert_eq!(
        detail(kuma(&edited, Fake::silent()).provision(&heartbeat()).await),
        "Uptime Kuma did not say which monitor it saved"
    );
    let (unanswered, _) = serving(answered(&json!({}), Vec::new())).await;
    assert!(detail(
        kuma(&unanswered, Fake::silent())
            .provision(&heartbeat())
            .await
    )
    .contains("did not answer over its Socket.IO connection"));
}

#[tokio::test]
async fn a_heartbeat_is_not_made_without_an_unpredictable_token() {
    let (base, packets) = serving(answered(
        &json!({}),
        vec![(
            "getTags",
            r#"43{id}[{"ok":true,"tags":[{"id":2,"name":"lemonfiber"}]}]"#,
        )],
    ))
    .await;
    for drawn in [None, Some(vec![1, 2, 3])] {
        let kuma = UptimeKuma::new(
            Fake::silent(),
            base.clone(),
            Arc::new("admin".to_owned()),
            Arc::new("Ab1-password".to_owned()),
            QUICK,
            Arc::new(Chance::exactly(drawn)),
        );
        assert_eq!(
            detail(kuma.provision(&heartbeat()).await),
            "no unpredictable bytes could be drawn for a push token"
        );
    }
    assert!(asked_with(&packets, "add").is_empty());
}

#[tokio::test]
async fn what_cannot_be_watched_or_is_not_lemonfiber_s_is_refused_before_uptime_kuma_is_asked() {
    let (base, packets) = serving(answered(&json!({}), Vec::new())).await;
    let kuma = kuma(&base, Fake::silent());
    let docker = Provision {
        kind: MonitorKind::Docker,
        ..heartbeat()
    };
    assert!(matches!(
        kuma.provision(&docker).await,
        Err(Failure::Disallowed { .. })
    ));
    for refused in [kuma.check_in("mine").await, kuma.retire("mine").await] {
        assert_eq!(detail(refused), "mine is not a tag of lemonfiber's");
    }
    assert!(heard(&packets).is_empty());
}

#[tokio::test]
async fn a_check_in_is_pushed_to_the_heartbeat_holding_the_tag() {
    let (base, _) = serving(answered(
        &json!({ "4": ours(4, TAG, "push"), "5": ours(5, TAG, "http") }),
        Vec::new(),
    ))
    .await;
    let pushed = Fake::always(Answer::reply(200, r#"{"ok":true}"#));
    assert!(kuma(&base, pushed.clone()).check_in(TAG).await.is_ok());
    assert_eq!(
        pushed
            .requests()
            .iter()
            .map(|asked| asked.url.clone())
            .collect::<Vec<_>>(),
        [format!("{base}/api/push/kept-token?status=up&msg=OK")]
    );
}

#[tokio::test]
async fn a_check_in_uptime_kuma_does_not_take_or_no_heartbeat_holds_is_refused() {
    let (base, _) = serving(answered(&json!({ "4": ours(4, TAG, "push") }), Vec::new())).await;
    assert_eq!(
        detail(
            kuma(
                &base,
                Fake::always(Answer::reply(404, r#"{"ok":false,"msg":"Monitor not found or not active."}"#))
            )
            .check_in(TAG)
            .await
        ),
        "Uptime Kuma did not take the check-in of lemonfiber:job:guard: Monitor not found or not active."
    );
    assert_eq!(
        detail(
            kuma(&base, Fake::always(Answer::reply(502, "bad gateway")))
                .check_in(TAG)
                .await
        ),
        "Uptime Kuma did not take the check-in of lemonfiber:job:guard: it answered 502"
    );
    assert!(matches!(
        kuma(&base, Fake::silent()).check_in(TAG).await,
        Err(Failure::Unavailable { .. })
    ));
    let mut tokenless = ours(4, TAG, "push");
    if let Some(fields) = tokenless.as_object_mut() {
        fields.insert("pushToken".to_owned(), json!(""));
    }
    let (empty, _) = serving(answered(
        &json!({ "4": tokenless, "5": ours(5, TAG, "http") }),
        Vec::new(),
    ))
    .await;
    let pushed = Fake::silent();
    assert_eq!(
        detail(kuma(&empty, pushed.clone()).check_in(TAG).await),
        "no heartbeat holds lemonfiber:job:guard"
    );
    assert!(pushed.requests().is_empty());
}

#[tokio::test]
async fn retiring_removes_every_monitor_holding_the_tag_and_nothing_where_none_does() {
    let listed = json!({
        "4": ours(4, TAG, "push"),
        "9": ours(9, TAG, "http"),
        "5": ours(5, "lemonfiber:job:other", "push"),
        "6": { "id": 6, "name": "mine", "type": "http", "tags": [] },
    });
    let (base, packets) = serving(answered(
        &listed,
        vec![("deleteMonitor", r#"43{id}[{"ok":true}]"#)],
    ))
    .await;
    assert!(kuma(&base, Fake::silent()).retire(TAG).await.is_ok());
    assert_eq!(
        asked_with(&packets, "deleteMonitor"),
        [json!([4, false]), json!([9, false])]
    );
    let (none, none_packets) = serving(answered(&json!({}), Vec::new())).await;
    assert!(kuma(&none, Fake::silent()).retire(TAG).await.is_ok());
    assert!(asked_with(&none_packets, "deleteMonitor").is_empty());
    let (refusing, _) = serving(answered(
        &listed,
        vec![("deleteMonitor", r#"43{id}[{"ok":false,"msg":"gone"}]"#)],
    ))
    .await;
    assert_eq!(
        detail(kuma(&refusing, Fake::silent()).retire(TAG).await),
        "Uptime Kuma refused deleteMonitor: gone"
    );
}
