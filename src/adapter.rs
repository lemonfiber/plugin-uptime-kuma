//! What this adapter says it is, and the kit that serves its contract.

use std::sync::Arc;

use lemonfiber_adapter_kit::{Kit, Readiness};
use lemonfiber_contract::adapter::{About, Release};
use lemonfiber_contract::capabilities::{owner, status};

use crate::uptime_kuma::UptimeKuma;

/// Where Uptime Kuma answers, by its service id and the port it listens on.
pub const ADDRESS: &str = "http://uptime-kuma:3001";

/// The upstream this adapter fronts, as its project names itself.
pub const UPSTREAM: &str = "Uptime Kuma";

include!(concat!(env!("OUT_DIR"), "/fronted.rs"));

/// What this adapter says it is: the contracts it speaks and the release it supports.
#[must_use]
pub fn about() -> About {
    About {
        speaks: vec![
            lemonfiber_contract::spoken(owner::claim::CAPABILITY, owner::claim::MAJOR),
            lemonfiber_contract::spoken(status::watch::CAPABILITY, status::watch::MAJOR),
        ],
        upstream: UPSTREAM.to_owned(),
        releases: vec![Release {
            version: RELEASE.to_owned(),
            digest: DIGEST.to_owned(),
        }],
    }
}

/// The contracts, served from `uptime_kuma` under `key`, ready while it answers.
#[must_use]
pub fn kit(key: String, uptime_kuma: Arc<UptimeKuma>) -> Kit {
    let asked = Arc::clone(&uptime_kuma);
    let ready: Readiness = Arc::new(move || {
        let asked = Arc::clone(&asked);
        Box::pin(async move { asked.answers().await })
    });
    Kit::new(key, about())
        .ready_when(ready)
        .serving(owner::claim::served(Arc::clone(&uptime_kuma)))
        .serving(status::watch::served(uptime_kuma))
}

#[cfg(test)]
mod tests;
