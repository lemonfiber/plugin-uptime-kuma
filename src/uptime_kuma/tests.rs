use std::sync::{Arc, Mutex};
use std::time::Duration;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::service::{Failure, Owner, Ownership as _};

use super::UptimeKuma;
use crate::socket::tests::{serving, Script};

const USERNAME: &str = "0123456789abcdef0123456789abcdef";

const PASSWORD: &str = "Ab1-cdefghijklmnopqrstuvwxyz0123";

const QUICK: Duration = Duration::from_millis(500);

const SET_UP: (&str, &str) = ("needSetup", r"43{id}[false]");

const WAITING: (&str, &str) = ("needSetup", r"43{id}[true]");

const OURS: (&str, &str) = ("login", r#"43{id}[{"ok":true,"token":"jwt"}]"#);

const THEIRS: (&str, &str) = (
    "login",
    r#"43{id}[{"ok":false,"msg":"authIncorrectCreds","msgi18n":true}]"#,
);

struct Missing;

impl lemonfiber_adapter_kit::credential::Holding for Missing {
    fn key(&self) -> Result<String, String> {
        Err("upstream.json could not be read: not found".to_owned())
    }
}

fn kuma(base: &str) -> UptimeKuma {
    UptimeKuma::new(
        Fake::silent(),
        base,
        Arc::new(USERNAME.to_owned()),
        Arc::new(PASSWORD.to_owned()),
        QUICK,
    )
}

fn script(answers: &[(&'static str, &str)]) -> Script {
    Script {
        answers: answers
            .iter()
            .map(|(event, answer)| (*event, (*answer).to_owned()))
            .collect(),
        ..Script::default()
    }
}

fn heard(packets: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    packets
        .lock()
        .map(|heard| heard.clone())
        .unwrap_or_default()
}

fn refused_saying(outcome: Result<Owner, Failure>, said: &str) -> bool {
    matches!(outcome, Err(Failure::Refused { detail, .. }) if detail.contains(said))
}

#[tokio::test]
async fn it_answers_while_uptime_kuma_answers_its_entry_page() {
    let up = Fake::always(Answer::reply(200, r#"{"type":"entryPage"}"#));
    let kuma = UptimeKuma::new(
        up.clone(),
        "http://uptime-kuma:3001",
        Arc::new(USERNAME.to_owned()),
        Arc::new(PASSWORD.to_owned()),
        QUICK,
    );
    assert!(kuma.answers().await);
    assert!(up
        .requests()
        .iter()
        .all(|asked| asked.url == "http://uptime-kuma:3001/api/entry-page"));
    for down in [Fake::silent(), Fake::always(Answer::reply(503, ""))] {
        let kuma = UptimeKuma::new(
            down,
            "http://uptime-kuma:3001",
            Arc::new(USERNAME.to_owned()),
            Arc::new(PASSWORD.to_owned()),
            QUICK,
        );
        assert!(!kuma.answers().await);
    }
}

#[tokio::test]
async fn an_uptime_kuma_nothing_answers_at_is_settling() {
    let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await else {
        return;
    };
    let free = listener
        .local_addr()
        .map(|a| format!("http://{a}"))
        .unwrap_or_default();
    drop(listener);
    assert_eq!(kuma(&free).owner().await.ok(), Some(Owner::Settling));
    assert_eq!(kuma(&free).claim().await.ok(), Some(Owner::Settling));
}

#[tokio::test]
async fn an_uptime_kuma_waiting_to_be_set_up_is_open() {
    let (base, packets) = serving(script(&[WAITING])).await;
    assert_eq!(kuma(&base).owner().await.ok(), Some(Owner::Open));
    assert!(!heard(&packets)
        .iter()
        .any(|packet| packet.contains("login")));
}

#[tokio::test]
async fn a_set_up_uptime_kuma_is_ours_where_the_minted_administrator_signs_in() {
    let (base, packets) = serving(script(&[SET_UP, OURS])).await;
    assert_eq!(kuma(&base).owner().await.ok(), Some(Owner::Ours));
    assert!(heard(&packets).contains(&format!(
        r#"422["login",{{"password":"{PASSWORD}","token":"","username":"{USERNAME}"}}]"#
    )));
    let (asking, _) = serving(script(&[
        SET_UP,
        ("login", r#"43{id}[{"tokenRequired":true}]"#),
    ]))
    .await;
    assert_eq!(kuma(&asking).owner().await.ok(), Some(Owner::Ours));
}

#[tokio::test]
async fn a_set_up_uptime_kuma_the_minted_administrator_cannot_sign_in_to_is_theirs() {
    let (base, _) = serving(script(&[SET_UP, THEIRS])).await;
    assert_eq!(kuma(&base).owner().await.ok(), Some(Owner::Theirs));
    assert_eq!(kuma(&base).claim().await.ok(), Some(Owner::Theirs));
}

#[tokio::test]
async fn a_part_already_held_is_answered_as_it_is_and_never_set_up_again() {
    let (base, packets) = serving(script(&[SET_UP, OURS])).await;
    assert_eq!(kuma(&base).claim().await.ok(), Some(Owner::Ours));
    assert!(!heard(&packets)
        .iter()
        .any(|packet| packet.contains(r#""setup""#)));
}

#[tokio::test]
async fn an_open_uptime_kuma_is_set_up_with_the_minted_administrator() {
    let (base, packets) = serving(script(&[
        WAITING,
        (
            "setup",
            r#"43{id}[{"ok":true,"msg":"successAdded","msgi18n":true}]"#,
        ),
        OURS,
    ]))
    .await;
    assert_eq!(kuma(&base).claim().await.ok(), Some(Owner::Ours));
    assert!(heard(&packets).contains(&format!(r#"422["setup","{USERNAME}","{PASSWORD}"]"#)));
}

#[tokio::test]
async fn a_set_up_whose_administrator_then_does_not_sign_in_is_refused() {
    let (base, _) = serving(script(&[
        WAITING,
        ("setup", r#"43{id}[{"ok":true}]"#),
        THEIRS,
    ]))
    .await;
    assert!(refused_saying(
        kuma(&base).claim().await,
        "does not sign it in"
    ));
}

#[tokio::test]
async fn a_set_up_uptime_kuma_refuses_while_still_waiting_is_refused_with_its_reason() {
    let (base, _) = serving(script(&[
        WAITING,
        (
            "setup",
            r#"43{id}[{"ok":false,"msg":"passwordTooWeak","msgi18n":true}]"#,
        ),
    ]))
    .await;
    assert!(refused_saying(kuma(&base).claim().await, "passwordTooWeak"));
    let (silent, _) = serving(script(&[WAITING, ("setup", r#"43{id}[{"ok":false}]"#)])).await;
    assert!(refused_saying(
        kuma(&silent).claim().await,
        "no reason given"
    ));
}

#[tokio::test]
async fn a_set_up_another_caller_finished_first_is_answered_as_whoever_holds_it() {
    let (base, _) = serving(script(&[
        WAITING,
        SET_UP,
        ("setup", r#"43{id}[{"ok":false,"msg":"initialized"}]"#),
        THEIRS,
    ]))
    .await;
    assert_eq!(kuma(&base).claim().await.ok(), Some(Owner::Theirs));
}

#[tokio::test]
async fn an_answer_that_is_not_the_one_uptime_kuma_gives_is_refused() {
    let (base, _) = serving(script(&[("needSetup", r#"43{id}["yes"]"#)])).await;
    assert!(refused_saying(
        kuma(&base).owner().await,
        "did not say whether it waits"
    ));
    let (unanswered, _) = serving(script(&[])).await;
    assert!(refused_saying(
        kuma(&unanswered).owner().await,
        "did not answer over its Socket.IO connection"
    ));
    let (opened_wrong, _) = serving(Script {
        opening: "nope".to_owned(),
        ..Script::default()
    })
    .await;
    assert!(refused_saying(
        kuma(&opened_wrong).claim().await,
        "did not answer over its Socket.IO connection"
    ));
}

#[tokio::test]
async fn a_socket_that_ends_mid_claim_is_refused() {
    for answers in [
        vec![WAITING],
        vec![WAITING, ("setup", r#"43{id}[{"ok":true}]"#)],
        vec![SET_UP],
    ] {
        let (base, _) = serving(script(&answers)).await;
        assert!(kuma(&base).claim().await.is_err());
    }
}

#[tokio::test]
async fn without_the_minted_administrator_nothing_is_set_up_or_signed_in() {
    let (base, packets) = serving(script(&[WAITING, SET_UP])).await;
    for (username, password) in [
        (
            Arc::new(Missing) as Arc<dyn lemonfiber_adapter_kit::credential::Holding>,
            Arc::new(PASSWORD.to_owned()) as Arc<dyn lemonfiber_adapter_kit::credential::Holding>,
        ),
        (Arc::new(USERNAME.to_owned()), Arc::new(Missing)),
    ] {
        let kuma = UptimeKuma::new(Fake::silent(), base.clone(), username, password, QUICK);
        assert!(refused_saying(
            kuma.claim().await,
            "this adapter holds no administrator for Uptime Kuma"
        ));
    }
    assert!(!heard(&packets)
        .iter()
        .any(|packet| packet.contains("setup\"") || packet.contains("login")));
}

#[tokio::test]
async fn a_set_up_uptime_kuma_is_not_signed_in_to_without_the_minted_administrator() {
    let (base, _) = serving(script(&[SET_UP])).await;
    let kuma = UptimeKuma::new(
        Fake::silent(),
        base,
        Arc::new(Missing),
        Arc::new(PASSWORD.to_owned()),
        QUICK,
    );
    assert!(refused_saying(
        kuma.owner().await,
        "this adapter holds no administrator for Uptime Kuma"
    ));
}
