//! What a provision sets on the Uptime Kuma monitor holding its tag, and what a monitor
//! Uptime Kuma is handed to make starts from.

use lemonfiber_ports::service::{Covers, MonitorKind, Provision, Statuses};
use serde_json::{json, Map, Value};

/// The monitor type Uptime Kuma calls a heartbeat a job checks in to.
pub const PUSH: &str = "push";

/// The statuses Uptime Kuma's own form reads an address up on.
const SUCCESS: Statuses = Statuses { from: 200, to: 299 };

/// The resolver a DNS monitor asks: the one Docker answers in every container on a
/// network it made.
const RESOLVER: &str = "127.0.0.11";

/// The port a DNS monitor asks its resolver on.
const DNS_PORT: u16 = 53;

/// How many pings a ping monitor sends at each check, as Uptime Kuma's own form sets it.
const PINGS: u32 = 3;

/// The seconds a ping monitor waits for each ping, as Uptime Kuma's own form sets it.
const PING_WAIT_S: u64 = 2;

/// The seconds a ping monitor waits for all of them, as Uptime Kuma's own form sets it.
const PING_TIMEOUT_S: u64 = 10;

/// The bytes a ping carries, as Uptime Kuma's own form sets it.
const PING_BYTES: u32 = 56;

/// The share of its interval a check waits for an answer, in fifths, as Uptime Kuma's own
/// form sets it.
const TIMEOUT_FIFTHS: u64 = 4;

/// Why a provision for a container cannot be made a monitor.
const NO_DOCKER: &str = "Uptime Kuma here is given no Docker host, so it cannot watch a container";

/// Why a provision for a word in an answer cannot be made a monitor.
const NO_WORD: &str = "a keyword monitor names a word to look for, and a provision names none";

/// Every field `provision` sets on its monitor, or why it cannot be made one.
///
/// # Errors
///
/// Why, where its tag is not one of lemonfiber's, it asks for a container or a word, or a
/// port check's target names no port.
pub fn shaped(provision: &Provision) -> Result<Map<String, Value>, String> {
    if Covers::read(&provision.tag).is_none() {
        return Err(format!("{} is not a tag of lemonfiber's", provision.tag));
    }
    let every = provision.every_s;
    let checked = || {
        json!({
            "interval": every,
            "retryInterval": every,
            "maxretries": provision.retries.saturating_sub(1),
            "timeout": every.saturating_mul(TIMEOUT_FIFTHS) / 5,
        })
    };
    let (kind, fields) = match provision.kind {
        MonitorKind::Docker => return Err(NO_DOCKER.to_owned()),
        MonitorKind::Keyword => return Err(NO_WORD.to_owned()),
        MonitorKind::Http => (
            "http",
            json!({
                "url": provision.target,
                "accepted_statuscodes": accepting(provision.accepted.as_deref().unwrap_or(&[SUCCESS])),
            }),
        ),
        MonitorKind::Port => {
            let (hostname, port) = hosted(&provision.target)
                .ok_or_else(|| format!("{} names no host and port", provision.target))?;
            ("port", json!({ "hostname": hostname, "port": port }))
        }
        MonitorKind::Ping => (
            "ping",
            json!({
                "hostname": provision.target,
                "ping_count": PINGS,
                "ping_numeric": true,
                "packetSize": PING_BYTES,
                "ping_per_request_timeout": PING_WAIT_S,
                "timeout": PING_TIMEOUT_S,
            }),
        ),
        MonitorKind::Dns => (
            "dns",
            json!({
                "hostname": provision.target,
                "dns_resolve_server": RESOLVER,
                "dns_resolve_type": "A",
                "port": DNS_PORT,
            }),
        ),
        MonitorKind::Heartbeat => (
            PUSH,
            json!({
                "interval": every.saturating_add(provision.grace_s.unwrap_or_default()),
                "retryInterval": every,
                "maxretries": provision.retries,
            }),
        ),
    };
    let mut shaped = Map::new();
    shaped.insert("type".to_owned(), Value::from(kind));
    if provision.kind != MonitorKind::Heartbeat {
        merge(&mut shaped, checked());
    }
    merge(&mut shaped, fields);
    Ok(shaped)
}

/// Every field Uptime Kuma's own form starts a monitor with that a provision does not set.
const FRESH: &str = r#"{
    "parent": null,
    "url": "",
    "wsSubprotocol": "",
    "method": "GET",
    "protocol": null,
    "location": "world",
    "ipFamily": null,
    "resendInterval": 0,
    "retryOnlyOnStatusCodeFailure": false,
    "notificationIDList": {},
    "ignoreTls": false,
    "upsideDown": false,
    "expiryNotification": false,
    "domainExpiryNotification": true,
    "maxredirects": 10,
    "saveResponse": false,
    "saveErrorResponse": true,
    "responseMaxLength": 1024,
    "dns_resolve_type": "A",
    "dns_resolve_server": "",
    "docker_container": "",
    "docker_host": null,
    "proxyId": null,
    "basic_auth_user": "",
    "basic_auth_pass": "",
    "bearer_token": "",
    "mqttUsername": "",
    "mqttPassword": "",
    "mqttTopic": "",
    "mqttWebsocketPath": "",
    "mqttSuccessMessage": "",
    "mqttCheckType": "keyword",
    "authMethod": null,
    "oauth_auth_method": "client_secret_basic",
    "httpBodyEncoding": "json",
    "kafkaProducerBrokers": [],
    "kafkaProducerSaslOptions": {"mechanism": "None"},
    "cacheBust": false,
    "kafkaProducerSsl": false,
    "kafkaProducerAllowAutoTopicCreation": false,
    "gamedigGivenPortOnly": true,
    "gamedigToken": "",
    "remote_browser": null,
    "screenshot_delay": 0,
    "rabbitmqNodes": [],
    "rabbitmqUsername": "",
    "rabbitmqPassword": "",
    "conditions": [],
    "system_service_name": "",
    "sshAuthMethod": "password",
    "ntpStratumThreshold": 5,
    "ntpTimeOffsetThreshold": 1000,
    "ntpRootDispersionThreshold": 500
}"#;

/// The monitor Uptime Kuma is handed to make, named `name`, before what a provision sets.
#[must_use]
pub fn fresh(name: &str) -> Map<String, Value> {
    let mut fresh: Map<String, Value> = serde_json::from_str(FRESH).unwrap_or_default();
    fresh.insert("name".to_owned(), Value::from(name));
    fresh.insert("accepted_statuscodes".to_owned(), accepting(&[SUCCESS]));
    fresh
}

/// `accepted`, as Uptime Kuma writes a status or an inclusive range of them.
fn accepting(accepted: &[Statuses]) -> Value {
    accepted
        .iter()
        .map(|statuses| {
            if statuses.from == statuses.to {
                statuses.from.to_string()
            } else {
                format!("{}-{}", statuses.from, statuses.to)
            }
        })
        .collect()
}

/// `target`'s host and port, a bracketed IPv6 host unbracketed.
fn hosted(target: &str) -> Option<(&str, u16)> {
    let (host, port) = target.rsplit_once(':')?;
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    let port = port.parse().ok()?;
    (!host.is_empty()).then_some((host, port))
}

/// `fields`, set on `shaped`.
fn merge(shaped: &mut Map<String, Value>, fields: Value) {
    if let Value::Object(fields) = fields {
        shaped.extend(fields);
    }
}

#[cfg(test)]
mod tests;
