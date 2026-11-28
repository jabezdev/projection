use anyhow::{anyhow, Context, Result};
use protocol::model::{new_id, Change, EditStep, ItemContent, Service, ServiceItem, Song};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::Repository;

const UNDO_DEPTH: i64 = 100;
const MERGE_WINDOW_MS: i64 = 1_000;

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct History {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redo: Option<String>,
}

#[derive(Clone, Copy)]
enum Stack {
    Undo,
    Redo,
}

impl Stack {
    fn name(self) -> &'static str {
        match self {
            Stack::Undo => "undo",
            Stack::Redo => "redo",
        }
    }

    fn order(self) -> &'static str {
        match self {
            Stack::Undo => "DESC",
            Stack::Redo => "ASC",
        }
    }
}

impl Repository {
    pub fn history(&self, record_id: &str) -> Result<History> {
        let label = |stack| Ok::<_, anyhow::Error>(self.top(record_id, stack)?.and_then(|(_, step)| step.label));
        Ok(History { undo: label(Stack::Undo)?, redo: label(Stack::Redo)? })
    }

    pub fn edit_log(&self, record_id: &str) -> Result<Vec<EditStep>> {
        let mut statement = self.conn.prepare("SELECT body FROM edit_step WHERE record_id = ?1 ORDER BY seq")?;
        let rows = statement.query_map(params![record_id], |row| row.get::<_, String>(0))?;
        rows.map(|body| Ok(serde_json::from_str(&body?)?)).collect()
    }

    pub fn compacted_through(&self, record_id: &str) -> Result<Option<i64>> {
        Ok(self.conn.query_row("SELECT through_seq FROM edit_compaction WHERE record_id = ?1", params![record_id], |row| row.get(0)).optional()?)
    }

    pub fn undo_service(&self, id: &str, now_ms: i64) -> Result<Option<Service>> {
        self.atomic(|| self.step_service(id, Stack::Undo, now_ms))
    }

    pub fn redo_service(&self, id: &str, now_ms: i64) -> Result<Option<Service>> {
        self.atomic(|| self.step_service(id, Stack::Redo, now_ms))
    }

    pub fn undo_song(&self, id: &str, now_ms: i64) -> Result<Option<Song>> {
        self.atomic(|| self.step_song(id, Stack::Undo, now_ms))
    }

    pub fn redo_song(&self, id: &str, now_ms: i64) -> Result<Option<Song>> {
        self.atomic(|| self.step_song(id, Stack::Redo, now_ms))
    }

    fn step_service(&self, id: &str, stack: Stack, now_ms: i64) -> Result<Option<Service>> {
        let Some((seq, step)) = self.top(id, stack)? else { return Ok(None) };
        let mut service = self.service(id)?.ok_or_else(|| anyhow!("service {id} not found"))?;
        let before = service.clone();
        let change = self.replay(seq, &step, stack, now_ms)?;
        change.apply_to_service(&mut service);
        self.write_service(&service, Some(&before), now_ms)?;
        self.mark_dirty(protocol::sync::RecordKind::Service, id)?;
        Ok(Some(service))
    }

    fn step_song(&self, id: &str, stack: Stack, now_ms: i64) -> Result<Option<Song>> {
        let Some((seq, step)) = self.top(id, stack)? else { return Ok(None) };
        let mut song = self.song(id)?.ok_or_else(|| anyhow!("song {id} not found"))?;
        let change = self.replay(seq, &step, stack, now_ms)?;
        change.apply_to_song(&mut song);
        self.write_song(&song, now_ms).map(Some)
    }

    fn replay(&self, seq: i64, step: &EditStep, stack: Stack, now_ms: i64) -> Result<Change> {
        let label = step.label.clone().unwrap_or_default();
        let (change, reverts, label, moved_to) = match stack {
            Stack::Undo => (step.change.inverted(), Some(step.id.clone()), format!("Undo {}", lowercase_first(&label)), Stack::Redo),
            Stack::Redo => (step.change.clone(), None, label, Stack::Undo),
        };
        let replayed = EditStep { id: new_id(), at_ms: now_ms, label: Some(label), reverts, change: change.clone() };
        self.insert_step(&replayed, "done")?;
        self.conn.execute("UPDATE edit_step SET stack = ?2 WHERE seq = ?1", params![seq, moved_to.name()])?;
        Ok(change)
    }

    pub(super) fn record_service_edit(&self, before: &Service, after: &Service, now_ms: i64) -> Result<()> {
        let change = Change::Service { id: after.id.clone(), ops: protocol::model::diff_service(before, after) };
        self.record(change, now_ms, &|change| change.describe(&|item_id| self.item_name(before, after, item_id)))
    }

    pub(super) fn record_song_edit(&self, before: &Song, after: &Song, now_ms: i64) -> Result<()> {
        let change = Change::Song { id: after.id.clone(), ops: protocol::model::diff_song(before, after) };
        self.record(change, now_ms, &|change| change.describe(&|id| id.to_string()))
    }

    pub(super) fn drop_history(&self, record_id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM edit_step WHERE record_id = ?1", params![record_id])?;
        self.conn.execute("DELETE FROM edit_compaction WHERE record_id = ?1", params![record_id])?;
        Ok(())
    }

    fn record(&self, change: Change, now_ms: i64, describe: &dyn Fn(&Change) -> String) -> Result<()> {
        if change.is_empty() {
            return Ok(());
        }
        let record_id = change.record_id().to_string();
        let redo_waiting = self.top(&record_id, Stack::Redo)?.is_some();
        self.conn.execute("UPDATE edit_step SET stack = 'done' WHERE record_id = ?1 AND stack = 'redo'", params![record_id])?;
        if let Some((seq, mut top)) = self.top(&record_id, Stack::Undo)?.filter(|(_, top)| !redo_waiting && now_ms - top.at_ms <= MERGE_WINDOW_MS) {
            if let Some(merged) = top.change.merge(&change) {
                if merged.is_empty() {
                    self.conn.execute("DELETE FROM edit_step WHERE seq = ?1", params![seq])?;
                } else {
                    top.label = Some(describe(&merged));
                    top.change = merged;
                    top.at_ms = now_ms;
                    self.conn.execute("UPDATE edit_step SET at_ms = ?2, body = ?3 WHERE seq = ?1", params![seq, now_ms, serde_json::to_string(&top)?])?;
                }
                return Ok(());
            }
        }
        let step = EditStep { id: new_id(), at_ms: now_ms, label: Some(describe(&change)), reverts: None, change };
        self.insert_step(&step, "undo")?;
        self.compact(&record_id)
    }

    fn compact(&self, record_id: &str) -> Result<()> {
        let cutoff: Option<i64> = self
            .conn
            .query_row(
                "SELECT seq FROM edit_step WHERE record_id = ?1 AND stack = 'undo' ORDER BY seq DESC LIMIT 1 OFFSET ?2",
                params![record_id, UNDO_DEPTH],
                |row| row.get(0),
            )
            .optional()?;
        let Some(cutoff) = cutoff else { return Ok(()) };
        self.conn.execute("DELETE FROM edit_step WHERE record_id = ?1 AND seq <= ?2", params![record_id, cutoff])?;
        self.conn.execute(
            "INSERT INTO edit_compaction(record_id, through_seq) VALUES (?1, ?2) ON CONFLICT(record_id) DO UPDATE SET through_seq = ?2",
            params![record_id, cutoff],
        )?;
        Ok(())
    }

    fn insert_step(&self, step: &EditStep, stack: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO edit_step(id, record_id, at_ms, stack, body) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![step.id, step.change.record_id(), step.at_ms, stack, serde_json::to_string(step)?],
            )
            .context("record edit")?;
        Ok(())
    }

    fn top(&self, record_id: &str, stack: Stack) -> Result<Option<(i64, EditStep)>> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT seq, body FROM edit_step WHERE record_id = ?1 AND stack = ?2 ORDER BY seq {} LIMIT 1", stack.order()),
                params![record_id, stack.name()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        row.map(|(seq, body)| Ok((seq, serde_json::from_str(&body).context("decode edit")?))).transpose()
    }

    fn item_name(&self, before: &Service, after: &Service, item_id: &str) -> String {
        let item = after.items.iter().chain(&before.items).find(|i| i.id == item_id);
        item.map(|item| self.describe_item(item)).unwrap_or_else(|| "an item".into())
    }

    fn describe_item(&self, item: &ServiceItem) -> String {
        if let Some(title) = item.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            return title.to_string();
        }
        match &item.content {
            ItemContent::Song(song) => self.song(&song.song_id).ok().flatten().map(|s| s.title).unwrap_or_else(|| "a song".into()),
            ItemContent::Media { media_id, .. } => self.media_file(media_id).ok().flatten().map(|m| m.title).unwrap_or_else(|| "a media item".into()),
            ItemContent::Presentation { .. } => "a presentation".into(),
            ItemContent::Capture { .. } => "a capture".into(),
            ItemContent::Text { steps } => steps.first().map(|s| s.lines().next().unwrap_or_default().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "a text item".into()),
            ItemContent::Scripture { reference, .. } => if reference.trim().is_empty() { "a Bible passage".into() } else { reference.trim().to_string() },
            ItemContent::Header => "a header".into(),
        }
    }
}

fn lowercase_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}
