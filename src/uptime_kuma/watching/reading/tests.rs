use std::collections::BTreeMap;

use lemonfiber_ports::service::{Beat, Monitor, MonitorKind, MonitorState};
use serde_json::{json, Value};

use super::{instant, monitors, tag_name, tagged};

const NOW: u64 = 1_800_000_000;

/// 2026-11-08 12:00:00 UTC.
const NOON: u64 = 1_794_139_200;

fn beat(status: u64, time: &str, important: &Value) -> Value {
    json!({ "status": status, "time": time, "important": important })
}

fn one(monitor: &Value, beats: &[Value]) -> Option<Monitor> {
    let id = monitor
        .get("id")
        .map(ToString::to_string)
        .unwrap_or_default();
    let listed = json!({ id.clone(): monitor });
    let beats = BTreeMap::from([(id, beats.to_vec())]);
    monitors(&listed, &beats, NOW).into_iter().next()
}

fn ours(tag: &str) -> Value {
    json!([{ "tag_id": 1, "monitor_id": 1, "name": "lemonfiber", "value": tag, "color": "#D97706" }])
}

#[test]
fn the_tag_is_named_for_lemonfiber_s_own_prefix() {
    assert_eq!(tag_name(), "lemonfiber");
}

#[test]
fn a_check_lemonfiber_made_is_read_with_its_tag_its_state_and_since_its_last_change() {
    let monitor = json!({
        "id": 4, "name": "Sonarr", "type": "http", "url": "http://sonarr:8989/ping",
        "active": true, "maintenance": false, "interval": 60, "retryInterval": 60,
        "tags": ours("lemonfiber:service:sonarr"),
    });
    let beats = [
        beat(0, "2026-11-08 11:58:00.000", &json!(1)),
        beat(1, "2026-11-08 11:59:00.250", &json!(true)),
        beat(1, "2026-11-08 12:00:00.000", &json!(false)),
    ];
    assert_eq!(
        one(&monitor, &beats),
        Some(Monitor {
            id: "4".to_owned(),
            tag: Some("lemonfiber:service:sonarr".to_owned()),
            name: "Sonarr".to_owned(),
            kind: MonitorKind::Http,
            target: "http://sonarr:8989/ping".to_owned(),
            active: true,
            heartbeat: None,
            state: MonitorState::Up,
            since: NOON - 60,
            last_reading: Some(NOON),
        })
    );
}

#[test]
fn a_monitor_with_no_beat_is_pending_since_it_was_read_and_with_no_change_since_its_oldest() {
    let monitor = json!({ "id": 1, "name": "n", "type": "port", "hostname": "sabnzbd", "port": 8080, "active": true });
    let unread = one(&monitor, &[]);
    assert_eq!(
        unread
            .as_ref()
            .map(|read| (read.state, read.since, read.last_reading)),
        Some((MonitorState::Pending, NOW, None))
    );
    assert_eq!(
        unread.map(|read| read.target),
        Some("sabnzbd:8080".to_owned())
    );
    let unchanged = one(
        &monitor,
        &[
            beat(2, "2026-11-08 11:59:00", &json!(0)),
            beat(2, "2026-11-08T12:00:00Z", &Value::Null),
        ],
    );
    assert_eq!(
        unchanged.map(|read| (read.state, read.since)),
        Some((MonitorState::Pending, NOON - 60))
    );
}

#[test]
fn a_down_beat_reads_down_and_a_paused_monitor_or_one_in_maintenance_reads_paused() {
    let active = json!({ "id": 1, "name": "n", "type": "http", "active": true });
    let state = |monitor: &Value, status: u64| {
        one(
            monitor,
            &[beat(status, "2026-11-08 12:00:00", &json!(true))],
        )
        .map(|read| read.state)
    };
    assert_eq!(state(&active, 0), Some(MonitorState::Down));
    assert_eq!(state(&active, 3), Some(MonitorState::Paused));
    assert_eq!(state(&active, 9), Some(MonitorState::Pending));
    let paused = json!({ "id": 1, "name": "n", "type": "http", "active": false });
    assert_eq!(state(&paused, 1), Some(MonitorState::Paused));
    let maintained =
        json!({ "id": 1, "name": "n", "type": "http", "active": true, "maintenance": true });
    assert_eq!(state(&maintained, 1), Some(MonitorState::Paused));
}

#[test]
fn a_heartbeat_reads_its_interval_its_grace_its_last_check_in_and_the_job_its_tag_names() {
    let monitor = json!({
        "id": 2, "name": "lemonfiber:job:guard", "type": "push", "pushToken": "secret",
        "active": true, "interval": 120, "retryInterval": 60, "tags": ours("lemonfiber:job:guard"),
    });
    let read = one(
        &monitor,
        &[
            beat(1, "2026-11-08 11:59:00", &json!(true)),
            beat(0, "2026-11-08 12:00:00", &json!(true)),
        ],
    );
    assert_eq!(
        read.as_ref()
            .map(|read| (read.kind, read.target.as_str(), read.heartbeat)),
        Some((
            MonitorKind::Heartbeat,
            "guard",
            Some(Beat {
                every_s: 60,
                grace_s: 60,
                last_check_in: Some(NOON - 60),
            })
        ))
    );
    let theirs = json!({ "id": 3, "name": "backup", "type": "push", "pushToken": "secret", "active": true, "interval": 60, "retryInterval": 120 });
    assert_eq!(
        one(&theirs, &[]).map(|read| (read.target, read.tag, read.heartbeat)),
        Some((
            String::new(),
            None,
            Some(Beat {
                every_s: 120,
                grace_s: 0,
                last_check_in: None,
            })
        ))
    );
}

#[test]
fn every_type_is_read_as_its_nearest_kind_at_what_it_watches_and_a_group_is_left_out() {
    let cases = [
        (
            json!({ "type": "real-browser", "url": "https://a" }),
            MonitorKind::Http,
            "https://a",
        ),
        (
            json!({ "type": "keyword", "url": "https://k" }),
            MonitorKind::Keyword,
            "https://k",
        ),
        (
            json!({ "type": "json-query", "url": "https://j" }),
            MonitorKind::Keyword,
            "https://j",
        ),
        (
            json!({ "type": "grpc-keyword", "grpcUrl": "g:1" }),
            MonitorKind::Keyword,
            "g:1",
        ),
        (
            json!({ "type": "ping", "hostname": "p" }),
            MonitorKind::Ping,
            "p",
        ),
        (
            json!({ "type": "tailscale-ping", "hostname": "t" }),
            MonitorKind::Ping,
            "t",
        ),
        (
            json!({ "type": "globalping", "url": "https://g" }),
            MonitorKind::Ping,
            "https://g",
        ),
        (
            json!({ "type": "dns", "hostname": "d", "port": 53 }),
            MonitorKind::Dns,
            "d",
        ),
        (
            json!({ "type": "docker", "docker_container": "c" }),
            MonitorKind::Docker,
            "c",
        ),
        (
            json!({ "type": "mysql", "hostname": "db", "port": 3306 }),
            MonitorKind::Port,
            "db:3306",
        ),
        (
            json!({ "type": "mqtt", "hostname": "broker", "port": null }),
            MonitorKind::Port,
            "broker",
        ),
        (
            json!({ "type": "websocket-upgrade", "url": "wss://w" }),
            MonitorKind::Port,
            "wss://w",
        ),
        (json!({ "type": "steam" }), MonitorKind::Port, ""),
    ];
    for (mut monitor, kind, target) in cases {
        if let Some(fields) = monitor.as_object_mut() {
            fields.insert("id".to_owned(), json!(1));
            fields.insert("active".to_owned(), json!(true));
        }
        assert_eq!(
            one(&monitor, &[]).map(|read| (read.kind, read.target)),
            Some((kind, target.to_owned())),
            "{monitor}"
        );
    }
    assert_eq!(one(&json!({ "id": 1, "type": "group" }), &[]), None);
}

#[test]
fn every_monitor_is_read_in_the_order_of_its_id_and_one_with_no_id_is_left_out() {
    let listed = json!({
        "10": { "id": 10, "name": "ten", "type": "http", "active": true },
        "9": { "id": 9, "name": "nine", "type": "http", "active": true },
        "x": { "name": "none", "type": "http" },
    });
    let read: Vec<String> = monitors(&listed, &BTreeMap::new(), NOW)
        .into_iter()
        .map(|monitor| monitor.id)
        .collect();
    assert_eq!(read, ["9", "10"]);
    assert!(monitors(&Value::Null, &BTreeMap::new(), NOW).is_empty());
}

#[test]
fn only_a_lemonfiber_tag_under_the_lemonfiber_name_is_lemonfiber_s() {
    let monitor = json!({ "tags": [
        { "name": "lemonfiber", "value": "mine" },
        { "name": "lemonfiber", "value": null },
        { "name": "other", "value": "lemonfiber:service:sonarr" },
        { "name": "lemonfiber", "value": "lemonfiber:entry:jellyfin" },
    ]});
    assert_eq!(tagged(&monitor), Some("lemonfiber:entry:jellyfin"));
    assert_eq!(tagged(&json!({ "tags": [] })), None);
    assert_eq!(tagged(&json!({})), None);
}

#[test]
fn a_moment_is_read_only_where_it_is_one() {
    assert_eq!(instant("2026-11-08 12:00:00"), Some(NOON));
    assert_eq!(instant("2026-11-08T12:00:00.999Z"), Some(NOON));
    assert_eq!(instant("2026-11-08 12:00:00+00:00"), Some(NOON));
    assert_eq!(instant("1970-01-01 00:00:00"), Some(0));
    assert_eq!(instant("2024-02-29 00:00:00"), Some(1_709_164_800));
    assert_eq!(instant("2000-03-01 00:00:00"), Some(951_868_800));
    for wrong in [
        "",
        "2026-11-08",
        "2026-13-08 12:00:00",
        "2026-00-08 12:00:00",
        "2026-11-32 12:00:00",
        "2026-11-00 12:00:00",
        "2026-11-08 24:00:00",
        "2026-11-08 12:60:00",
        "2026-11-08 12:00:61",
        "2026-11-08 12:00",
        "2026-11 12:00:00",
        "2026-11-0x 12:00:00",
        "2026-11-08 1x:00:00",
        "1969-12-31 23:59:59",
    ] {
        assert_eq!(instant(wrong), None, "{wrong}");
    }
}
