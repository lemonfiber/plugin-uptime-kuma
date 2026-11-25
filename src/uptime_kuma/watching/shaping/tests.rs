use lemonfiber_ports::service::{MonitorKind, Provision, Statuses};
use serde_json::{json, Value};

use super::{fresh, shaped};

fn asked(kind: MonitorKind, target: &str) -> Provision {
    Provision {
        tag: "lemonfiber:service:sonarr".to_owned(),
        kind,
        target: target.to_owned(),
        every_s: 60,
        grace_s: None,
        retries: 3,
        accepted: None,
    }
}

fn shape(provision: &Provision) -> Value {
    shaped(provision).map_or(Value::Null, Value::Object)
}

#[test]
fn a_check_alarms_on_the_failed_check_it_names_and_waits_four_fifths_of_its_interval() {
    assert_eq!(
        shape(&asked(MonitorKind::Http, "http://sonarr:8989/ping")),
        json!({
            "type": "http",
            "url": "http://sonarr:8989/ping",
            "accepted_statuscodes": ["200-299"],
            "interval": 60,
            "retryInterval": 60,
            "maxretries": 2,
            "timeout": 48,
        })
    );
    let once = Provision {
        retries: 0,
        ..asked(MonitorKind::Http, "http://sonarr:8989")
    };
    assert_eq!(shape(&once).get("maxretries"), Some(&json!(0)));
}

#[test]
fn a_port_check_is_its_host_and_its_port_and_a_target_naming_neither_is_refused() {
    let port = shape(&asked(MonitorKind::Port, "sabnzbd:8080"));
    assert_eq!(
        (port.get("type"), port.get("hostname"), port.get("port")),
        (
            Some(&json!("port")),
            Some(&json!("sabnzbd")),
            Some(&json!(8080))
        )
    );
    let bracketed = shape(&asked(MonitorKind::Port, "[fd00::1]:53"));
    assert_eq!(bracketed.get("hostname"), Some(&json!("fd00::1")));
    for target in ["sabnzbd", ":8080", "sabnzbd:port", "sabnzbd:99999"] {
        assert_eq!(
            shaped(&asked(MonitorKind::Port, target)),
            Err(format!("{target} names no host and port"))
        );
    }
}

#[test]
fn a_ping_and_a_name_are_checked_as_uptime_kuma_s_own_form_starts_them() {
    let ping = shape(&asked(MonitorKind::Ping, "gluetun"));
    assert_eq!(
        (
            ping.get("hostname"),
            ping.get("ping_count"),
            ping.get("ping_numeric"),
            ping.get("packetSize"),
            ping.get("ping_per_request_timeout"),
            ping.get("timeout"),
        ),
        (
            Some(&json!("gluetun")),
            Some(&json!(3)),
            Some(&json!(true)),
            Some(&json!(56)),
            Some(&json!(2)),
            Some(&json!(10)),
        )
    );
    let dns = shape(&asked(MonitorKind::Dns, "home.example"));
    assert_eq!(
        (
            dns.get("type"),
            dns.get("hostname"),
            dns.get("dns_resolve_server"),
            dns.get("dns_resolve_type"),
            dns.get("port"),
        ),
        (
            Some(&json!("dns")),
            Some(&json!("home.example")),
            Some(&json!("127.0.0.11")),
            Some(&json!("A")),
            Some(&json!(53)),
        )
    );
}

#[test]
fn a_heartbeat_waits_its_interval_and_grace_and_each_further_miss_one_interval() {
    let beat = Provision {
        tag: "lemonfiber:job:guard".to_owned(),
        kind: MonitorKind::Heartbeat,
        target: "guard".to_owned(),
        every_s: 60,
        grace_s: Some(30),
        retries: 1,
        accepted: None,
    };
    assert_eq!(
        shape(&beat),
        json!({ "type": "push", "interval": 90, "retryInterval": 60, "maxretries": 1 })
    );
    let graceless = Provision {
        grace_s: None,
        ..beat
    };
    assert_eq!(shape(&graceless).get("interval"), Some(&json!(60)));
}

#[test]
fn a_container_a_word_and_a_tag_not_lemonfiber_s_are_refused() {
    assert_eq!(
        shaped(&asked(MonitorKind::Docker, "lemonfiber-gluetun-1")),
        Err("Uptime Kuma here is given no Docker host, so it cannot watch a container".to_owned())
    );
    assert_eq!(
        shaped(&asked(MonitorKind::Keyword, "http://sonarr:8989")),
        Err("a keyword monitor names a word to look for, and a provision names none".to_owned())
    );
    for tag in [
        "mine",
        "lemonfiber:",
        "lemonfiber:service:",
        "lemonfiber:other:x",
    ] {
        let foreign = Provision {
            tag: tag.to_owned(),
            ..asked(MonitorKind::Http, "http://sonarr:8989")
        };
        assert_eq!(
            shaped(&foreign),
            Err(format!("{tag} is not a tag of lemonfiber's"))
        );
    }
}

#[test]
fn a_fresh_monitor_is_named_and_holds_every_field_uptime_kuma_s_form_starts_one_with() {
    let made = fresh("lemonfiber:service:sonarr");
    assert_eq!(made.get("name"), Some(&json!("lemonfiber:service:sonarr")));
    assert_eq!(made.get("accepted_statuscodes"), Some(&json!(["200-299"])));
    assert_eq!(made.get("notificationIDList"), Some(&json!({})));
    assert_eq!(made.get("conditions"), Some(&json!([])));
    assert!(made.get("type").is_none());
}

#[test]
fn an_address_reads_up_on_the_statuses_it_is_told_alone_and_on_success_where_it_is_told_none() {
    let told = Provision {
        accepted: Some(vec![
            Statuses { from: 200, to: 399 },
            Statuses::only(401),
            Statuses::only(403),
        ]),
        ..asked(MonitorKind::Http, "http://sonarr:8989/ping")
    };
    assert_eq!(
        shape(&told).get("accepted_statuscodes"),
        Some(&json!(["200-399", "401", "403"]))
    );
    assert_eq!(
        shape(&asked(MonitorKind::Http, "http://sonarr:8989/ping")).get("accepted_statuscodes"),
        Some(&json!(["200-299"]))
    );
}
