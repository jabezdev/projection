use serde_json::{json, Value};

use super::list::{self, ListChange};
use super::{ItemField, ServiceField, ServiceOp};
use crate::model::{ItemContent, Service, ServiceItem};

const FIELDS: [ServiceField; 5] = [ServiceField::ScheduledAtMs, ServiceField::Title, ServiceField::Leader, ServiceField::Notes, ServiceField::EndedAtMs];
const SONG_ITEM_FIELDS: [ItemField; 6] = [ItemField::Title, ItemField::Notes, ItemField::Background, ItemField::Key, ItemField::Arrangement, ItemField::VersionId];
const ITEM_FIELDS: [ItemField; 4] = [ItemField::Title, ItemField::Notes, ItemField::Background, ItemField::Content];

pub(super) fn diff(before: &Service, after: &Service) -> Vec<ServiceOp> {
    let mut ops: Vec<ServiceOp> = FIELDS
        .iter()
        .filter_map(|&field| {
            let (b, a) = (get(before, field), get(after, field));
            (b != a).then_some(ServiceOp::Set { field, before: b, after: a })
        })
        .collect();
    ops.extend(list::diff(&before.items, &after.items).into_iter().map(|change| match change {
        ListChange::Insert(item, prev) => ServiceOp::InsertItem { item, prev },
        ListChange::Remove(item, prev) => ServiceOp::RemoveItem { item, prev },
        ListChange::Move(id, from, to) => ServiceOp::MoveItem { id, from, to },
    }));
    for new in &after.items {
        let Some(old) = before.items.iter().find(|i| i.id == new.id) else { continue };
        for &field in item_fields(old, new) {
            let (b, a) = (get_item(old, field).unwrap_or(Value::Null), get_item(new, field).unwrap_or(Value::Null));
            if b != a {
                ops.push(ServiceOp::SetItem { id: new.id.clone(), field, before: b, after: a });
            }
        }
    }
    ops
}

fn item_fields(old: &ServiceItem, new: &ServiceItem) -> &'static [ItemField] {
    match (&old.content, &new.content) {
        (ItemContent::Song(a), ItemContent::Song(b)) if a.song_id == b.song_id => &SONG_ITEM_FIELDS,
        _ => &ITEM_FIELDS,
    }
}

impl ServiceOp {
    pub fn inverted(&self) -> ServiceOp {
        match self.clone() {
            ServiceOp::Set { field, before, after } => ServiceOp::Set { field, before: after, after: before },
            ServiceOp::InsertItem { item, prev } => ServiceOp::RemoveItem { item, prev },
            ServiceOp::RemoveItem { item, prev } => ServiceOp::InsertItem { item, prev },
            ServiceOp::MoveItem { id, from, to } => ServiceOp::MoveItem { id, from: to, to: from },
            ServiceOp::SetItem { id, field, before, after } => ServiceOp::SetItem { id, field, before: after, after: before },
        }
    }

    pub fn apply(&self, service: &mut Service) -> bool {
        match self {
            ServiceOp::Set { field, before, after } => {
                let clean = get(service, *field) == *before;
                set(service, *field, after) && clean
            }
            ServiceOp::InsertItem { item, prev } => list::insert(&mut service.items, item.clone(), prev.as_deref()),
            ServiceOp::RemoveItem { item, .. } => list::remove(&mut service.items, item),
            ServiceOp::MoveItem { id, from, to } => {
                let clean = list::predecessor(&service.items, id) == *from;
                list::move_to(&mut service.items, id, to.as_deref()) && clean
            }
            ServiceOp::SetItem { id, field, before, after } => {
                let Some(item) = service.items.iter_mut().find(|i| &i.id == id) else { return false };
                let clean = get_item(item, *field).as_ref() == Some(before);
                set_item(item, *field, after) && clean
            }
        }
    }
}

#[derive(PartialEq)]
enum Key<'a> {
    Field(ServiceField),
    Item(&'a str, ItemField),
}

fn key(op: &ServiceOp) -> Option<Key<'_>> {
    match op {
        ServiceOp::Set { field, .. } => Some(Key::Field(*field)),
        ServiceOp::SetItem { id, field, .. } => Some(Key::Item(id, *field)),
        _ => None,
    }
}

pub(super) fn merge(earlier: &[ServiceOp], later: &[ServiceOp]) -> Option<Vec<ServiceOp>> {
    if earlier.len() != later.len() {
        return None;
    }
    let mut merged = Vec::new();
    for old in earlier {
        let old_key = key(old)?;
        let new = later.iter().find(|op| key(op).as_ref() == Some(&old_key))?;
        let op = match (old, new) {
            (ServiceOp::Set { field, before, .. }, ServiceOp::Set { after, .. }) => ServiceOp::Set { field: *field, before: before.clone(), after: after.clone() },
            (ServiceOp::SetItem { id, field, before, .. }, ServiceOp::SetItem { after, .. }) => {
                ServiceOp::SetItem { id: id.clone(), field: *field, before: before.clone(), after: after.clone() }
            }
            _ => return None,
        };
        merged.push(op);
    }
    merged.retain(|op| !matches!(op, ServiceOp::Set { before, after, .. } | ServiceOp::SetItem { before, after, .. } if before == after));
    Some(merged)
}

fn get(service: &Service, field: ServiceField) -> Value {
    match field {
        ServiceField::ScheduledAtMs => json!(service.scheduled_at_ms),
        ServiceField::Title => json!(service.title),
        ServiceField::Leader => json!(service.leader),
        ServiceField::Notes => json!(service.notes),
        ServiceField::EndedAtMs => json!(service.ended_at_ms),
    }
}

fn set(service: &mut Service, field: ServiceField, value: &Value) -> bool {
    match field {
        ServiceField::ScheduledAtMs => read(value).map(|v| service.scheduled_at_ms = v).is_some(),
        ServiceField::Title => read(value).map(|v| service.title = v).is_some(),
        ServiceField::Leader => read(value).map(|v| service.leader = v).is_some(),
        ServiceField::Notes => read(value).map(|v| service.notes = v).is_some(),
        ServiceField::EndedAtMs => read(value).map(|v| service.ended_at_ms = v).is_some(),
    }
}

fn get_item(item: &ServiceItem, field: ItemField) -> Option<Value> {
    let song = match &item.content {
        ItemContent::Song(song) => Some(song),
        _ => None,
    };
    Some(match field {
        ItemField::Title => json!(item.title),
        ItemField::Notes => json!(item.notes),
        ItemField::Background => json!(item.background),
        ItemField::Content => json!(item.content),
        ItemField::Key => json!(song?.key),
        ItemField::Arrangement => json!(song?.arrangement),
        ItemField::VersionId => json!(song?.version_id),
    })
}

fn set_item(item: &mut ServiceItem, field: ItemField, value: &Value) -> bool {
    match field {
        ItemField::Title => read(value).map(|v| item.title = v).is_some(),
        ItemField::Notes => read(value).map(|v| item.notes = v).is_some(),
        ItemField::Background => read(value).map(|v| item.background = v).is_some(),
        ItemField::Content => read(value).map(|v| item.content = v).is_some(),
        ItemField::Key | ItemField::Arrangement | ItemField::VersionId => {
            let ItemContent::Song(song) = &mut item.content else { return false };
            match field {
                ItemField::Key => read(value).map(|v| song.key = v).is_some(),
                ItemField::Arrangement => read(value).map(|v| song.arrangement = v).is_some(),
                _ => read(value).map(|v| song.version_id = v).is_some(),
            }
        }
    }
}

pub(super) fn read<T: serde::de::DeserializeOwned>(value: &Value) -> Option<T> {
    serde_json::from_value(value.clone()).ok()
}
