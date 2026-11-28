use anyhow::{anyhow, Context, Result};
use protocol::model::{ItemContent, Service, ServiceItem};
use protocol::CaptureSource;
use rusqlite::{params, OptionalExtension};
use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

use protocol::sync::RecordKind as Kind;

use super::Repository;

#[derive(Serialize, Deserialize, Default)]
struct DeviceBinding {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    slides: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<CaptureSource>,
}

impl Repository {
    pub fn save_service(&self, service: &Service, now_ms: i64) -> Result<()> {
        self.atomic(|| {
            let old = self.service(&service.id)?;
            let frozen = self.is_frozen(&service.id)?;
            let mut service = service.clone();
            if frozen {
                self.pin_songs(&mut service)?;
            }
            if let Some(old) = old.as_ref().filter(|_| !frozen) {
                self.record_service_edit(old, &service, now_ms)?;
            }
            self.write_service(&service, old.as_ref(), now_ms)?;
            if old.as_ref() != Some(&service) {
                self.mark_dirty(Kind::Service, &service.id)?;
            }
            Ok(())
        })
    }

    pub(super) fn write_service(&self, service: &Service, old: Option<&Service>, now_ms: i64) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO service(id, scheduled_at_ms, title, leader, notes, ended_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET scheduled_at_ms = ?2, title = ?3, leader = ?4, notes = ?5, ended_at_ms = ?6, updated_at_ms = ?7",
                params![service.id, service.scheduled_at_ms, service.title, service.leader, service.notes, service.ended_at_ms, now_ms],
            )
            .context("save service")?;
        let old_items: HashMap<&str, (usize, &ServiceItem)> = old.map(|o| o.items.iter().enumerate().map(|(i, item)| (item.id.as_str(), (i, item))).collect()).unwrap_or_default();
        let kept: HashSet<&str> = service.items.iter().map(|i| i.id.as_str()).collect();
        for id in old_items.keys().filter(|id| !kept.contains(*id)) {
            self.conn.execute("DELETE FROM device_binding WHERE item_id = ?1", params![id])?;
            self.conn.execute("DELETE FROM service_item WHERE id = ?1", params![id])?;
        }
        for (position, item) in service.items.iter().enumerate() {
            if old_items.get(item.id.as_str()) != Some(&(position, item)) {
                self.write_item(&service.id, position, item)?;
            }
        }
        Ok(())
    }

    fn is_frozen(&self, id: &str) -> Result<bool> {
        Ok(self.conn.query_row("SELECT EXISTS(SELECT 1 FROM service WHERE id = ?1 AND frozen = 1)", params![id], |row| row.get(0))?)
    }

    pub fn service(&self, id: &str) -> Result<Option<Service>> {
        let row = self
            .conn
            .query_row("SELECT id, scheduled_at_ms, title, leader, notes, ended_at_ms FROM service WHERE id = ?1", params![id], |row| {
                Ok(Service { id: row.get(0)?, scheduled_at_ms: row.get(1)?, title: row.get(2)?, leader: row.get(3)?, notes: row.get(4)?, ended_at_ms: row.get(5)?, items: Vec::new() })
            })
            .optional()
            .context("get service")?;
        let Some(mut service) = row else { return Ok(None) };
        service.items = self.items_of(id)?;
        Ok(Some(service))
    }

    pub fn list_services(&self) -> Result<Vec<Service>> {
        let mut statement = self.conn.prepare("SELECT id FROM service ORDER BY scheduled_at_ms DESC, updated_at_ms DESC")?;
        let ids = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().filter_map(|id| self.service(id).transpose()).collect()
    }

    pub fn delete_service(&self, id: &str) -> Result<bool> {
        self.atomic(|| {
            let removed = self.remove_service(id)?;
            self.mark_deleted(Kind::Service, id)?;
            Ok(removed)
        })
    }

    pub(super) fn remove_service(&self, id: &str) -> Result<bool> {
        self.drop_history(id)?;
        self.conn.execute("DELETE FROM device_binding WHERE item_id IN (SELECT id FROM service_item WHERE service_id = ?1)", params![id])?;
        self.conn.execute("DELETE FROM service_item WHERE service_id = ?1", params![id])?;
        Ok(self.conn.execute("DELETE FROM service WHERE id = ?1", params![id]).context("delete service")? > 0)
    }

    pub fn end_service(&self, id: &str, now_ms: i64) -> Result<()> {
        let affected = self.conn.execute("UPDATE service SET ended_at_ms = ?2 WHERE id = ?1 AND ended_at_ms IS NULL", params![id, now_ms])?;
        if affected == 0 && self.service(id)?.is_none() {
            return Err(anyhow!("service {id} not found"));
        }
        self.freeze_services("WHERE id = ?1", params![id])?;
        self.mark_dirty(Kind::Service, id)
    }

    pub fn freeze_past_services(&self, today_starts_at_ms: i64) -> Result<usize> {
        self.freeze_services("WHERE frozen = 0 AND (ended_at_ms IS NOT NULL OR scheduled_at_ms < ?1)", params![today_starts_at_ms])
    }

    fn freeze_services(&self, filter: &str, args: impl rusqlite::Params) -> Result<usize> {
        let mut statement = self.conn.prepare(&format!("SELECT id FROM service {filter}"))?;
        let ids = statement.query_map(args, |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for id in &ids {
            let Some(old) = self.service(id)? else { continue };
            let mut service = old.clone();
            self.pin_songs(&mut service)?;
            let updated_at: i64 = self.conn.query_row("SELECT updated_at_ms FROM service WHERE id = ?1", params![id], |row| row.get(0))?;
            self.write_service(&service, Some(&old), updated_at)?;
            self.conn.execute("UPDATE service SET frozen = 1 WHERE id = ?1", params![id])?;
            self.drop_history(id)?;
            self.mark_dirty(Kind::Service, id)?;
        }
        Ok(ids.len())
    }

    fn pin_songs(&self, service: &mut Service) -> Result<()> {
        for item in &mut service.items {
            let ItemContent::Song(song_item) = &mut item.content else { continue };
            if song_item.version_id.is_none() {
                song_item.version_id = self.song(&song_item.song_id)?.map(|song| song.current.id);
            }
        }
        Ok(())
    }

    fn write_item(&self, service_id: &str, position: usize, item: &ServiceItem) -> Result<()> {
        let (song_id, version_id) = match &item.content {
            ItemContent::Song(song) => (Some(song.song_id.as_str()), song.version_id.as_deref()),
            _ => (None, None),
        };
        self.conn
            .execute(
                "INSERT INTO service_item(id, service_id, position, song_id, version_id, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET service_id = ?2, position = ?3, song_id = ?4, version_id = ?5, body = ?6",
                params![item.id, service_id, position as i64, song_id, version_id, serde_json::to_string(&ServiceItem { content: item.content.without_device_fields(), ..item.clone() })?],
            )
            .context("save service item")?;
        self.save_binding(item)
    }

    pub(super) fn save_binding(&self, item: &ServiceItem) -> Result<()> {
        let binding = match &item.content {
            ItemContent::Presentation { slides, .. } => DeviceBinding { slides: slides.clone(), source: None },
            ItemContent::Capture { source, .. } => DeviceBinding { slides: Vec::new(), source: source.clone() },
            _ => {
                self.conn.execute("DELETE FROM device_binding WHERE item_id = ?1", params![item.id])?;
                return Ok(());
            }
        };
        self.conn.execute(
            "INSERT INTO device_binding(item_id, body) VALUES (?1, ?2) ON CONFLICT(item_id) DO UPDATE SET body = ?2",
            params![item.id, serde_json::to_string(&binding)?],
        )?;
        Ok(())
    }

    pub(super) fn with_binding(&self, mut item: ServiceItem) -> Result<ServiceItem> {
        let body: Option<String> = self.conn.query_row("SELECT body FROM device_binding WHERE item_id = ?1", params![item.id], |row| row.get(0)).optional()?;
        let binding: DeviceBinding = body.map(|b| serde_json::from_str(&b)).transpose()?.unwrap_or_default();
        match &mut item.content {
            ItemContent::Presentation { slides, .. } if slides.is_empty() => *slides = binding.slides,
            ItemContent::Capture { source, .. } if source.is_none() => *source = binding.source,
            _ => {}
        }
        Ok(item)
    }

    fn items_of(&self, service_id: &str) -> Result<Vec<ServiceItem>> {
        let mut statement = self.conn.prepare(
            "SELECT i.body, b.body FROM service_item i LEFT JOIN device_binding b ON b.item_id = i.id WHERE i.service_id = ?1 ORDER BY i.position",
        )?;
        let rows = statement.query_map(params![service_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)))?;
        let mut items = Vec::new();
        for row in rows {
            let (body, binding) = row?;
            let mut item: ServiceItem = serde_json::from_str(&body).context("decode service item")?;
            let binding: DeviceBinding = binding.map(|b| serde_json::from_str(&b)).transpose()?.unwrap_or_default();
            match &mut item.content {
                ItemContent::Presentation { slides, .. } => *slides = binding.slides,
                ItemContent::Capture { source, .. } => *source = binding.source,
                _ => {}
            }
            items.push(item);
        }
        Ok(items)
    }
}
