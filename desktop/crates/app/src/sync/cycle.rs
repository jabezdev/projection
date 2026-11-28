use anyhow::Result;
use protocol::sync::{PullPage, Push, PushResult};
use serde_json::json;
use std::collections::HashSet;

use super::client::Convex;
use super::outbox;
use crate::{now_ms, Db};

const BATCH: usize = 25;

#[derive(Default)]
pub struct Outcome {
    pub changed: HashSet<String>,
    pub conflicts: usize,
}

pub async fn run(convex: &Convex, token: &str, db: &Db, keep_mine: Vec<(String, Push)>) -> Result<Outcome> {
    let mut outcome = Outcome::default();
    resolve(convex, token, db, keep_mine, &mut outcome).await?;
    push_deletes(convex, token, db).await?;
    push_changes(convex, token, db, &mut outcome).await?;
    pull(convex, token, db, &mut outcome).await?;
    Ok(outcome)
}

async fn send(convex: &Convex, token: &str, pushes: &[Push]) -> Result<Vec<PushResult>> {
    convex.mutation("sync:push", json!({ "token": token, "pushes": pushes })).await
}

async fn resolve(convex: &Convex, token: &str, db: &Db, keep_mine: Vec<(String, Push)>, outcome: &mut Outcome) -> Result<()> {
    for chunk in keep_mine.chunks(BATCH) {
        let pushes: Vec<Push> = chunk.iter().map(|(_, push)| push.clone()).collect();
        let results = send(convex, token, &pushes).await?;
        let db = db.lock()?;
        for ((conflict_id, _), result) in chunk.iter().zip(&results) {
            db.remove_conflict(conflict_id)?;
            if let Some(record) = &result.record {
                outcome.changed.extend(outbox::adopt(&db, result.kind, record, now_ms())?);
            }
            outcome.conflicts += outbox::record_conflicts(&db, result.kind, &result.id, &result.conflicts, now_ms())?;
        }
    }
    Ok(())
}

async fn push_deletes(convex: &Convex, token: &str, db: &Db) -> Result<()> {
    let deletes = outbox::deletes(&*db.lock()?)?;
    for chunk in deletes.chunks(BATCH) {
        send(convex, token, chunk).await?;
        let db = db.lock()?;
        for push in chunk {
            if let Push::Delete { kind, id } = push {
                db.delete_pushed(*kind, id)?;
            }
        }
    }
    Ok(())
}

async fn push_changes(convex: &Convex, token: &str, db: &Db, outcome: &mut Outcome) -> Result<()> {
    let (outgoing, unchanged) = outbox::changes(&*db.lock()?)?;
    {
        let db = db.lock()?;
        for record in unchanged {
            db.pushed(record.kind, &record.id, record.version, &record.base.unwrap_or_default())?;
        }
    }
    for chunk in outgoing.chunks(BATCH) {
        let pushes: Vec<Push> = chunk.iter().map(|o| o.push.clone()).collect();
        let results = send(convex, token, &pushes).await?;
        let db = db.lock()?;
        for (sent, result) in chunk.iter().zip(&results) {
            let (conflicts, changed) = outbox::settle(&db, &sent.record, result, now_ms())?;
            outcome.conflicts += conflicts;
            outcome.changed.extend(changed);
        }
    }
    Ok(())
}

async fn pull(convex: &Convex, token: &str, db: &Db, outcome: &mut Outcome) -> Result<()> {
    let mut cursor: f64 = db.lock()?.meta("sync_cursor")?.and_then(|c| c.parse().ok()).unwrap_or(0.0);
    for _ in 0..200 {
        let page: PullPage = convex.query("sync:pull", json!({ "token": token, "since": cursor })).await?;
        let db = db.lock()?;
        let applied = db.apply_pull(&page, now_ms())?;
        outcome.changed.extend(applied.changed);
        cursor = page.cursor.max(cursor);
        db.set_meta("sync_cursor", &cursor.to_string())?;
        if !page.more {
            break;
        }
    }
    db.lock()?.set_meta("sync_last_ms", &now_ms().to_string())?;
    Ok(())
}
