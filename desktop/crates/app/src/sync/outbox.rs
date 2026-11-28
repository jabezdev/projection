use anyhow::Result;
use persistence::{DirtyRecord, Repository};
use protocol::model::{diff_service, diff_song, new_id, Change, MediaFile, Service, ServiceTemplate, Song};
use protocol::sync::{Conflict, PullPage, Push, PushResult, RecordKind, RemoteMedia, RemoteService, RemoteSong};
use serde_json::{json, Value};

pub struct Outgoing {
    pub push: Push,
    pub record: DirtyRecord,
}

pub fn deletes(db: &Repository) -> Result<Vec<Push>> {
    Ok(db.deleted_records()?.into_iter().map(|(kind, id)| Push::Delete { kind, id }).collect())
}

pub fn changes(db: &Repository) -> Result<(Vec<Outgoing>, Vec<DirtyRecord>)> {
    let mut outgoing = Vec::new();
    let mut unchanged = Vec::new();
    for record in db.dirty_records()? {
        match build(db, &record)? {
            Some(push) => outgoing.push(Outgoing { push, record }),
            None => unchanged.push(record),
        }
    }
    Ok((outgoing, unchanged))
}

fn build(db: &Repository, record: &DirtyRecord) -> Result<Option<Push>> {
    let id = record.id.as_str();
    let kind = record.kind;
    let base = record.base.clone().filter(|b| !b.is_null());
    Ok(match kind {
        RecordKind::Service => {
            let Some(local) = db.wire_service(id)? else { return Ok(None) };
            match base.and_then(|b| serde_json::from_value::<Service>(b).ok()) {
                Some(base) => {
                    let ops = diff_service(&base, &local);
                    (!ops.is_empty()).then(|| Push::Change { change: Change::Service { id: id.into(), ops }, versions: vec![], current: None })
                }
                None => Some(Push::Create { kind, record: serde_json::to_value(&local)? }),
            }
        }
        RecordKind::Song => {
            let Some(local) = db.song(id)? else { return Ok(None) };
            let versions = db.pinned_versions(id)?;
            match base.and_then(|b| serde_json::from_value::<Song>(b).ok()) {
                Some(base) => {
                    let ops = diff_song(&base, &local);
                    let current = (base.current.id != local.current.id).then(|| local.current.id.clone());
                    (!ops.is_empty() || current.is_some()).then(|| Push::Change { change: Change::Song { id: id.into(), ops }, versions, current })
                }
                None => Some(Push::Create { kind, record: json!({ "song": local, "versions": versions }) }),
            }
        }
        RecordKind::Template => {
            let Some(local) = db.wire_template(id)? else { return Ok(None) };
            whole(kind, base, serde_json::to_value(local)?)
        }
        RecordKind::Media => {
            let Some(local) = db.wire_media(id)? else { return Ok(None) };
            whole(kind, base, serde_json::to_value(local)?)
        }
    })
}

fn whole(kind: RecordKind, base: Option<Value>, record: Value) -> Option<Push> {
    match base {
        Some(base) if base == record => None,
        Some(base) => Some(Push::Replace { kind, base, record }),
        None => Some(Push::Create { kind, record }),
    }
}

pub fn settle(db: &Repository, sent: &DirtyRecord, result: &PushResult, now_ms: i64) -> Result<(usize, Vec<String>)> {
    if result.error.as_deref() == Some("missing") {
        db.forget_base(sent.kind, &sent.id)?;
        return Ok((0, vec![]));
    }
    if let Some(error) = &result.error {
        log::warn!("sync: the server refused {} {}: {error}", sent.kind.name(), sent.id);
        return Ok((0, vec![]));
    }
    let record = result.record.clone().unwrap_or(Value::Null);
    db.pushed(sent.kind, &sent.id, sent.version, &record)?;
    let changed = if record.is_null() { vec![] } else { adopt(db, sent.kind, &record, now_ms)? };
    Ok((record_conflicts(db, sent.kind, &sent.id, &result.conflicts, now_ms)?, changed))
}

pub fn record_conflicts(db: &Repository, kind: RecordKind, id: &str, conflicts: &[protocol::sync::RemoteConflict], now_ms: i64) -> Result<usize> {
    let title = title_of(db, kind, id)?;
    for conflict in conflicts {
        db.add_conflict(&Conflict {
            id: new_id(),
            kind,
            record_id: id.into(),
            title: title.clone(),
            label: conflict.label.clone(),
            mine: conflict.mine.clone(),
            theirs: conflict.theirs.clone(),
            keep_mine: conflict.keep_mine.clone(),
            at_ms: now_ms,
        })?;
    }
    Ok(conflicts.len())
}

pub fn adopt(db: &Repository, kind: RecordKind, record: &Value, now_ms: i64) -> Result<Vec<String>> {
    let mut page = PullPage { cursor: 0.0, more: false, songs: vec![], services: vec![], templates: vec![], media: vec![], deleted: vec![], pinned: vec![] };
    match kind {
        RecordKind::Service => {
            let service: Service = serde_json::from_value(record.clone())?;
            let frozen = db.is_frozen_service(&service.id)?;
            page.services.push(RemoteService { service, frozen });
        }
        RecordKind::Song => page.songs.push(RemoteSong { song: serde_json::from_value(record.clone())?, versions: vec![] }),
        RecordKind::Template => page.templates.push(serde_json::from_value::<ServiceTemplate>(record.clone())?),
        RecordKind::Media => page.media.push(RemoteMedia { media: serde_json::from_value::<MediaFile>(record.clone())?, url: None, size: None }),
    }
    Ok(db.apply_pull(&page, now_ms)?.changed)
}

fn title_of(db: &Repository, kind: RecordKind, id: &str) -> Result<String> {
    Ok(match kind {
        RecordKind::Service => db.service(id)?.map(|s| s.title),
        RecordKind::Song => db.song(id)?.map(|s| s.title),
        RecordKind::Template => db.template(id)?.map(|t| t.name),
        RecordKind::Media => db.media_file(id)?.map(|m| m.title),
    }
    .unwrap_or_else(|| "A record".into()))
}
