use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use lemonfiber_contract::adapter::{about_path, ready_path, About};
use lemonfiber_fixtures::http::{Answer, Fake};
use tower::ServiceExt as _;

use super::{about, kit, ADDRESS, DIGEST, RELEASE, UPSTREAM};
use crate::uptime_kuma::UptimeKuma;

fn a_key() -> String {
    ('e'..='t').collect()
}

fn kuma(fake: &Arc<Fake>, base: &str) -> Arc<UptimeKuma> {
    Arc::new(UptimeKuma::new(
        fake.clone(),
        base,
        Arc::new(('q'..='x').collect::<String>()),
        Arc::new(('A'..='P').collect::<String>()),
        Duration::from_millis(300),
    ))
}

async fn answered_by(
    fake: &Arc<Fake>,
    base: &str,
    method: &str,
    path: &str,
    key: Option<&str>,
) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(key) = key {
        request = request.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    let Ok(request) = request.body(Body::from("{}")) else {
        return (StatusCode::IM_A_TEAPOT, Vec::new());
    };
    let Ok(response) = kit(a_key(), kuma(fake, base))
        .router()
        .oneshot(request)
        .await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .map(|bytes| bytes.to_vec())
        .unwrap_or_default();
    (status, bytes)
}

#[test]
fn it_speaks_the_claim_contract_and_names_the_release_its_recordings_were_taken_from() {
    let said = about();
    assert_eq!(said.speaks, ["owner.claim@1"]);
    assert_eq!(said.upstream, UPSTREAM);
    assert_eq!(
        said.releases
            .iter()
            .map(|release| (release.version.as_str(), release.digest.as_str()))
            .collect::<Vec<_>>(),
        [(RELEASE, DIGEST)]
    );
}

#[tokio::test]
async fn about_is_answered_under_the_plugin_s_key() {
    let key = a_key();
    let (status, body) =
        answered_by(&Fake::silent(), ADDRESS, "GET", &about_path(), Some(&key)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_slice::<About>(&body).ok(), Some(about()));
}

#[tokio::test]
async fn nothing_is_answered_without_the_key_or_under_another() {
    for key in [None, Some("not-the-plugin-s-key")] {
        for (method, path) in [
            ("POST", "/lemonfiber/owner.claim/v1/claim".to_owned()),
            ("GET", about_path()),
        ] {
            let fake = Fake::silent();
            assert_eq!(
                answered_by(&fake, ADDRESS, method, &path, key).await.0,
                StatusCode::UNAUTHORIZED
            );
            assert!(fake.requests().is_empty());
        }
    }
}

#[tokio::test]
async fn an_uptime_kuma_nothing_answers_at_is_settling() {
    let key = a_key();
    let (status, body) = answered_by(
        &Fake::silent(),
        "http://127.0.0.1:9",
        "POST",
        "/lemonfiber/owner.claim/v1/owner",
        Some(&key),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).ok(),
        Some(serde_json::json!("settling"))
    );
}

#[tokio::test]
async fn a_capability_it_does_not_speak_is_refused_as_unknown() {
    let key = a_key();
    let (status, _) = answered_by(
        &Fake::silent(),
        ADDRESS,
        "POST",
        "/lemonfiber/media.serve/v1/rescan",
        Some(&key),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn it_is_ready_while_uptime_kuma_answers_its_entry_page() {
    let key = a_key();
    let up = Fake::always(Answer::reply(200, "{}"));
    let (status, body) = answered_by(&up, ADDRESS, "GET", &ready_path(), Some(&key)).await;
    assert_eq!(
        (status, body.as_slice()),
        (StatusCode::OK, b"true".as_slice())
    );
    assert!(up
        .requests()
        .iter()
        .all(|asked| asked.url == "http://uptime-kuma:3001/api/entry-page"));
    for down in [Fake::silent(), Fake::always(Answer::reply(503, ""))] {
        let (status, body) = answered_by(&down, ADDRESS, "GET", &ready_path(), Some(&key)).await;
        assert_eq!(
            (status, body.as_slice()),
            (StatusCode::OK, b"false".as_slice())
        );
    }
}
