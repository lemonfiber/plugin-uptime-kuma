//! What this adapter says it is, and the kit that serves its contract.

use std::sync::Arc;

use lemonfiber_adapter_kit::{Kit, Readiness};
use lemonfiber_contract::adapter::{About, Release};
use lemonfiber_contract::capabilities::owner;

use crate::uptime_kuma::UptimeKuma;

/// Where Uptime Kuma answers, by its service id and the port it listens on.
pub const ADDRESS: &str = "http://uptime-kuma:3001";

/// The upstream this adapter fronts, as its project names itself.
pub const UPSTREAM: &str = "Uptime Kuma";

/// The Uptime Kuma release this adapter's recordings were taken from.
pub const RELEASE: &str = "2.5.4";

/// The digest of the image that release's recordings were taken from.
pub const DIGEST: &str = "sha256:917318f9d7be5257f43ba412c766a473be336eb451d70744f3b482d0c3997c0e";

/// What this adapter says it is: the contract it speaks and the release it supports.
#[must_use]
pub fn about() -> About {
    About {
        speaks: vec![lemonfiber_contract::spoken(
            owner::claim::CAPABILITY,
            owner::claim::MAJOR,
        )],
        upstream: UPSTREAM.to_owned(),
        releases: vec![Release {
            version: RELEASE.to_owned(),
            digest: DIGEST.to_owned(),
        }],
    }
}

/// The contract, served from `uptime_kuma` under `key`, ready while it answers.
#[must_use]
pub fn kit(key: String, uptime_kuma: Arc<UptimeKuma>) -> Kit {
    let asked = Arc::clone(&uptime_kuma);
    let ready: Readiness = Arc::new(move || {
        let asked = Arc::clone(&asked);
        Box::pin(async move { asked.answers().await })
    });
    Kit::new(key, about())
        .ready_when(ready)
        .serving(owner::claim::served(uptime_kuma))
}

#[cfg(test)]
mod tests;
