use anyhow::Result;
use protocol::sync::NowShowing;
use protocol::{EngineSnapshot, FeedDisplay, FeedState};
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, Instant};

use super::client::Convex;

const CLAIM_EVERY: Duration = Duration::from_secs(30);
const SHOW_EVERY: Duration = Duration::from_millis(700);

#[derive(Default)]
pub struct Live {
    claimed: Option<String>,
    claimed_at: Option<Instant>,
    shown: Option<Option<NowShowing>>,
    shown_at: Option<Instant>,
    pub held_by: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claim {
    live: bool,
    #[serde(default)]
    held_by: Option<String>,
}

pub fn live_service(snapshot: &EngineSnapshot, today_starts_at_ms: i64) -> Option<String> {
    let service = &snapshot.service.as_ref()?.service;
    (!service.is_past(today_starts_at_ms)).then(|| service.id.clone())
}

pub fn now_showing(snapshot: &EngineSnapshot) -> Option<NowShowing> {
    let service = &snapshot.service.as_ref()?.service;
    let feed = FeedState::from_snapshot(snapshot);
    Some(NowShowing {
        service_id: service.id.clone(),
        service_title: service.title.clone(),
        item_title: feed.item.and_then(|i| i.title),
        section: feed.section,
        text: feed.text,
        next: feed.next_text,
        blank: feed.display != FeedDisplay::Live,
    })
}

impl Live {
    pub async fn claim(&mut self, convex: &Convex, token: &str, wanted: Option<String>) -> Result<()> {
        let due = self.claimed != wanted || self.claimed_at.is_none_or(|at| at.elapsed() >= CLAIM_EVERY);
        if !due || (wanted.is_none() && self.claimed.is_none() && self.claimed_at.is_some()) {
            return Ok(());
        }
        let claim: Claim = convex.mutation("sync:claimLive", json!({ "token": token, "serviceId": wanted })).await?;
        self.held_by = if claim.live { None } else { claim.held_by };
        self.claimed = wanted;
        self.claimed_at = Some(Instant::now());
        Ok(())
    }

    pub async fn show(&mut self, convex: &Convex, token: &str, now: Option<NowShowing>) -> Result<()> {
        if self.shown.as_ref() == Some(&now) || self.shown_at.is_some_and(|at| at.elapsed() < SHOW_EVERY) {
            return Ok(());
        }
        convex.mutation::<serde_json::Value>("sync:showing", json!({ "token": token, "now": now })).await?;
        self.shown = Some(now);
        self.shown_at = Some(Instant::now());
        Ok(())
    }

    pub fn reset(&mut self) {
        *self = Live::default();
    }

    pub fn claimed(&self) -> Option<&str> {
        self.claimed.as_deref()
    }
}
