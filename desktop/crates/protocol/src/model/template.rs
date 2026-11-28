use serde::{Deserialize, Serialize};

use super::{new_id, ItemContent, Service, ServiceItem};

pub const DEFAULT_TEMPLATE_ID: &str = "sunday";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceTemplate {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekday: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_minutes: Option<u16>,
    #[serde(default)]
    pub parts: Vec<TemplatePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePart {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub items: Vec<ServiceItem>,
}

impl ServiceTemplate {
    pub fn sunday() -> ServiceTemplate {
        serde_json::from_str(include_str!("../../../../fixtures/sunday-template.json")).expect("valid default template")
    }

    pub fn default_parts(&self) -> Vec<String> {
        self.parts.iter().filter(|part| !part.optional).map(|part| part.id.clone()).collect()
    }

    pub fn instantiate(&self, parts: &[String], title: Option<&str>, scheduled_at_ms: i64) -> Service {
        let items = self.parts.iter().filter(|part| parts.contains(&part.id)).flat_map(|part| {
            let header = ServiceItem { id: new_id(), title: Some(part.title.clone()), notes: part.notes.clone(), background: None, content: ItemContent::Header };
            std::iter::once(header).chain(part.items.iter().map(fresh_item))
        });
        let title = title.map(str::trim).filter(|t| !t.is_empty()).map(str::to_string).or_else(|| self.title.clone()).unwrap_or_else(|| self.name.clone());
        Service { id: new_id(), scheduled_at_ms, title, leader: None, notes: None, ended_at_ms: None, items: items.collect() }
    }

    pub fn from_service(id: String, name: String, service: &Service, keep_songs: bool) -> ServiceTemplate {
        let mut parts: Vec<TemplatePart> = Vec::new();
        for item in &service.items {
            if let ItemContent::Header = item.content {
                parts.push(TemplatePart { id: new_id(), title: item.title.clone().unwrap_or_default(), optional: false, notes: item.notes.clone(), items: Vec::new() });
                continue;
            }
            if matches!(item.content, ItemContent::Song(_)) && !keep_songs {
                continue;
            }
            if parts.is_empty() {
                parts.push(TemplatePart { id: new_id(), title: "Start".into(), optional: false, notes: None, items: Vec::new() });
            }
            parts.last_mut().expect("a part").items.push(fresh_item(item));
        }
        ServiceTemplate { id, name, title: Some(service.title.clone()), weekday: None, start_minutes: None, parts }
    }
}

fn fresh_item(item: &ServiceItem) -> ServiceItem {
    let mut copy = item.clone();
    copy.id = new_id();
    if let ItemContent::Song(song) = &mut copy.content {
        song.version_id = None;
    }
    copy
}
