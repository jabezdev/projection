use anyhow::{Context, Result};
use protocol::model::{ItemContent, ServiceItem, ServiceTemplate};
use rusqlite::{params, OptionalExtension};

use protocol::sync::RecordKind as Kind;
use super::Repository;

impl Repository {
    pub fn list_templates(&self) -> Result<Vec<ServiceTemplate>> {
        let mut statement = self.conn.prepare("SELECT id FROM service_template ORDER BY name COLLATE NOCASE, id")?;
        let ids = statement.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().filter_map(|id| self.template(id).transpose()).collect()
    }

    pub fn template(&self, id: &str) -> Result<Option<ServiceTemplate>> {
        let Some(mut template) = self.wire_template(id)? else { return Ok(None) };
        for part in &mut template.parts {
            part.items = std::mem::take(&mut part.items).into_iter().map(|item| self.with_binding(item)).collect::<Result<_>>()?;
        }
        Ok(Some(template))
    }

    pub fn wire_template(&self, id: &str) -> Result<Option<ServiceTemplate>> {
        let body = self.conn.query_row("SELECT body FROM service_template WHERE id = ?1", params![id], |row| row.get::<_, String>(0)).optional()?;
        body.map(|b| serde_json::from_str(&b).context("decode template")).transpose()
    }

    pub fn save_template(&self, template: &ServiceTemplate, now_ms: i64) -> Result<()> {
        self.atomic(|| {
            self.write_template(template, now_ms)?;
            self.mark_dirty(Kind::Template, &template.id)
        })
    }

    pub(super) fn write_template(&self, template: &ServiceTemplate, now_ms: i64) -> Result<()> {
        let mut wire = template.clone();
        for part in &mut wire.parts {
            for item in &mut part.items {
                if binds(item) {
                    self.save_binding(item)?;
                }
                item.content = item.content.without_device_fields();
            }
        }
        let template = &wire;
        self.conn
            .execute(
                "INSERT INTO service_template(id, name, body, updated_at_ms) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET name = ?2, body = ?3, updated_at_ms = ?4",
                params![template.id, template.name, serde_json::to_string(template)?, now_ms],
            )
            .context("save template")?;
        Ok(())
    }

    pub fn delete_template(&self, id: &str) -> Result<bool> {
        self.atomic(|| {
            let removed = self.conn.execute("DELETE FROM service_template WHERE id = ?1", params![id])? > 0;
            if removed {
                self.mark_deleted(Kind::Template, id)?;
            }
            Ok(removed)
        })
    }

    pub fn seed_default_template(&self, now_ms: i64) -> Result<()> {
        if self.meta("templates_seeded")?.is_some() {
            return Ok(());
        }
        self.atomic(|| {
            if self.template(protocol::model::DEFAULT_TEMPLATE_ID)?.is_none() {
                self.write_template(&ServiceTemplate::sunday(), now_ms)?;
            }
            self.set_meta("templates_seeded", "1")
        })
    }
}

fn binds(item: &ServiceItem) -> bool {
    match &item.content {
        ItemContent::Presentation { slides, .. } => !slides.is_empty(),
        ItemContent::Capture { source, .. } => source.is_some(),
        _ => false,
    }
}
