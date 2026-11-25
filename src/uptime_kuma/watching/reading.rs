//! Every monitor Uptime Kuma holds, read from the list and the beats it emits to its
//! administrator as it signs in.

use std::collections::BTreeMap;

use lemonfiber_ports::service::{Beat, Covers, Monitor, MonitorKind, MonitorState, TAG_PREFIX};
use serde_json::Value;

use super::shaping::PUSH;

/// The monitor type Uptime Kuma calls a group, which watches nothing itself.
const GROUP: &str = "group";

/// What Uptime Kuma says a beat found.
const DOWN: u64 = 0;

const UP: u64 = 1;

const MAINTENANCE: u64 = 3;

/// The name of the Uptime Kuma tag whose value is lemonfiber's tag for a monitor.
#[must_use]
pub fn tag_name() -> &'static str {
    TAG_PREFIX.trim_end_matches(':')
}

/// Every monitor in `listed`, Uptime Kuma's list keyed by id, with each one's beats in
/// `beats`, oldest first, by its id; a group left out. `now` is what a monitor with no
/// beat is said to stand since.
#[must_use]
pub fn monitors(listed: &Value, beats: &BTreeMap<String, Vec<Value>>, now: u64) -> Vec<Monitor> {
    let mut read: Vec<(u64, Monitor)> = listed
        .as_object()
        .into_iter()
        .flat_map(|listed| listed.values())
        .filter_map(|monitor| {
            let id = monitor.get("id").and_then(Value::as_u64)?;
            let beats = beats
                .get(&id.to_string())
                .map(Vec::as_slice)
                .unwrap_or_default();
            Some((id, read(monitor, id, beats, now)?))
        })
        .collect();
    read.sort_by_key(|(id, _)| *id);
    read.into_iter().map(|(_, monitor)| monitor).collect()
}

/// The value of the Uptime Kuma tag `lemonfiber` on `monitor`, where it is one of
/// lemonfiber's tags.
#[must_use]
pub fn tagged(monitor: &Value) -> Option<&str> {
    monitor
        .get("tags")
        .and_then(Value::as_array)?
        .iter()
        .filter(|tag| tag.get("name").and_then(Value::as_str) == Some(tag_name()))
        .filter_map(|tag| tag.get("value").and_then(Value::as_str))
        .find(|value| Covers::read(value).is_some())
}

/// One monitor, by its `id`, with its `beats`; nothing for a group.
fn read(monitor: &Value, id: u64, beats: &[Value], now: u64) -> Option<Monitor> {
    let text = |field: &str| {
        monitor
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    let number = |field: &str| {
        monitor
            .get(field)
            .and_then(Value::as_u64)
            .unwrap_or_default()
    };
    let kind = kind(text("type"))?;
    let tag = tagged(monitor).map(str::to_owned);
    let active = monitor.get("active").and_then(Value::as_bool) == Some(true);
    let newest = beats.last();
    let state = if !active || monitor.get("maintenance").and_then(Value::as_bool) == Some(true) {
        MonitorState::Paused
    } else {
        newest.map_or(MonitorState::Pending, |beat| {
            match beat.get("status").and_then(Value::as_u64) {
                Some(DOWN) => MonitorState::Down,
                Some(UP) => MonitorState::Up,
                Some(MAINTENANCE) => MonitorState::Paused,
                _ => MonitorState::Pending,
            }
        })
    };
    let since = beats
        .iter()
        .rev()
        .find(|beat| important(beat))
        .or_else(|| beats.first())
        .and_then(at)
        .unwrap_or(now);
    let heartbeat = (kind == MonitorKind::Heartbeat).then(|| {
        let every_s = number("retryInterval");
        Beat {
            every_s,
            grace_s: number("interval").saturating_sub(every_s),
            last_check_in: beats
                .iter()
                .rev()
                .find(|beat| beat.get("status").and_then(Value::as_u64) == Some(UP))
                .and_then(at),
        }
    });
    let host = text("hostname");
    let target = match (text("type"), kind) {
        (PUSH, _) => tag
            .as_deref()
            .and_then(Covers::read)
            .map(|covers| covers.id().to_owned())
            .unwrap_or_default(),
        ("grpc-keyword", _) => text("grpcUrl").to_owned(),
        (_, MonitorKind::Docker) => text("docker_container").to_owned(),
        (_, MonitorKind::Http | MonitorKind::Keyword) => text("url").to_owned(),
        _ if host.is_empty() => text("url").to_owned(),
        (_, MonitorKind::Port) => match monitor.get("port").and_then(Value::as_u64) {
            Some(port) => format!("{host}:{port}"),
            None => host.to_owned(),
        },
        _ => host.to_owned(),
    };
    Some(Monitor {
        id: id.to_string(),
        tag,
        name: text("name").to_owned(),
        kind,
        target,
        active,
        heartbeat,
        state,
        since,
        last_reading: newest.and_then(at),
    })
}

/// The kind of a monitor Uptime Kuma calls `kind`: a port check for every type with no
/// nearer one, and nothing for a group.
fn kind(kind: &str) -> Option<MonitorKind> {
    Some(match kind {
        GROUP => return None,
        "http" | "real-browser" => MonitorKind::Http,
        "keyword" | "json-query" | "grpc-keyword" => MonitorKind::Keyword,
        "ping" | "tailscale-ping" | "globalping" => MonitorKind::Ping,
        "dns" => MonitorKind::Dns,
        "docker" => MonitorKind::Docker,
        PUSH => MonitorKind::Heartbeat,
        _ => MonitorKind::Port,
    })
}

/// Whether Uptime Kuma marked `beat` a change of state.
fn important(beat: &Value) -> bool {
    match beat.get("important") {
        Some(Value::Bool(important)) => *important,
        Some(Value::Number(important)) => important.as_u64() == Some(1),
        _ => false,
    }
}

/// When `beat` was taken, in seconds since the Unix epoch.
fn at(beat: &Value) -> Option<u64> {
    instant(beat.get("time")?.as_str()?)
}

/// `text`, a UTC moment as Uptime Kuma writes one, `YYYY-MM-DD HH:MM:SS` with any
/// fraction after it, in seconds since the Unix epoch.
fn instant(text: &str) -> Option<u64> {
    let (date, time) = text.split_once([' ', 'T'])?;
    let mut date = date.splitn(3, '-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let whole = time.split(['.', 'Z', '+']).next()?;
    let mut time = whole.splitn(3, ':').map(str::parse::<i64>);
    let (hour, minute, second) = (time.next()?.ok()?, time.next()?.ok()?, time.next()?.ok()?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..24).contains(&hour)
        || !(0..60).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }
    let seconds = days(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second;
    u64::try_from(seconds).ok()
}

/// The days from the Unix epoch to a date of the proleptic Gregorian calendar.
fn days(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_cycle - 719_468
}

#[cfg(test)]
mod tests;
