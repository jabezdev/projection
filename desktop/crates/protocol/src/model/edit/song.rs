use serde_json::{json, Value};

use super::list::{self, ListChange};
use super::service::read;
use super::{SongField, SongOp};
use crate::model::Song;

const FIELDS: [SongField; 7] = [SongField::Title, SongField::Archived, SongField::Credits, SongField::Key, SongField::Tags, SongField::Background, SongField::Arrangement];

pub(super) fn diff(before: &Song, after: &Song) -> Vec<SongOp> {
    let mut ops: Vec<SongOp> = FIELDS
        .iter()
        .filter(|&&field| field != SongField::Arrangement)
        .filter_map(|&field| set_op(before, after, field))
        .collect();
    ops.extend(list::diff(&before.current.sections, &after.current.sections).into_iter().map(|change| match change {
        ListChange::Insert(section, prev) => SongOp::InsertSection { section, prev },
        ListChange::Remove(section, prev) => SongOp::RemoveSection { section, prev },
        ListChange::Move(id, from, to) => SongOp::MoveSection { id, from, to },
    }));
    for new in &after.current.sections {
        let Some(old) = before.current.section(&new.id) else { continue };
        if old != new {
            ops.push(SongOp::SetSection { id: new.id.clone(), before: old.clone(), after: new.clone() });
        }
    }
    ops.extend(set_op(before, after, SongField::Arrangement));
    ops
}

fn set_op(before: &Song, after: &Song, field: SongField) -> Option<SongOp> {
    let (b, a) = (get(before, field), get(after, field));
    (b != a).then_some(SongOp::Set { field, before: b, after: a })
}

impl SongOp {
    pub fn inverted(&self) -> SongOp {
        match self.clone() {
            SongOp::Set { field, before, after } => SongOp::Set { field, before: after, after: before },
            SongOp::InsertSection { section, prev } => SongOp::RemoveSection { section, prev },
            SongOp::RemoveSection { section, prev } => SongOp::InsertSection { section, prev },
            SongOp::MoveSection { id, from, to } => SongOp::MoveSection { id, from: to, to: from },
            SongOp::SetSection { id, before, after } => SongOp::SetSection { id, before: after, after: before },
        }
    }

    pub fn apply(&self, song: &mut Song) -> bool {
        let sections = &mut song.current.sections;
        match self {
            SongOp::Set { field, before, after } => {
                let clean = get(song, *field) == *before;
                set(song, *field, after) && clean
            }
            SongOp::InsertSection { section, prev } => list::insert(sections, section.clone(), prev.as_deref()),
            SongOp::RemoveSection { section, .. } => list::remove(sections, section),
            SongOp::MoveSection { id, from, to } => {
                let clean = list::predecessor(sections, id) == *from;
                list::move_to(sections, id, to.as_deref()) && clean
            }
            SongOp::SetSection { id, before, after } => {
                let Some(section) = sections.iter_mut().find(|s| &s.id == id) else { return false };
                let clean = section == before;
                *section = after.clone();
                clean
            }
        }
    }
}

pub(super) fn merge(earlier: &[SongOp], later: &[SongOp]) -> Option<Vec<SongOp>> {
    if earlier.len() != later.len() {
        return None;
    }
    let mut merged = Vec::new();
    for old in earlier {
        let op = match old {
            SongOp::Set { field, before, .. } => {
                let after = later.iter().find_map(|op| match op {
                    SongOp::Set { field: f, after, .. } if f == field => Some(after),
                    _ => None,
                })?;
                (before != after).then(|| SongOp::Set { field: *field, before: before.clone(), after: after.clone() })
            }
            SongOp::SetSection { id, before, .. } => {
                let after = later.iter().find_map(|op| match op {
                    SongOp::SetSection { id: i, after, .. } if i == id => Some(after),
                    _ => None,
                })?;
                (before != after).then(|| SongOp::SetSection { id: id.clone(), before: before.clone(), after: after.clone() })
            }
            _ => return None,
        };
        merged.extend(op);
    }
    Some(merged)
}

fn get(song: &Song, field: SongField) -> Value {
    match field {
        SongField::Title => json!(song.title),
        SongField::Archived => json!(song.archived),
        SongField::Credits => json!(song.credits),
        SongField::Key => json!(song.key),
        SongField::Tags => json!(song.tags),
        SongField::Background => json!(song.current.background),
        SongField::Arrangement => json!(song.current.arrangement),
    }
}

fn set(song: &mut Song, field: SongField, value: &Value) -> bool {
    match field {
        SongField::Title => read(value).map(|v| song.title = v).is_some(),
        SongField::Archived => read(value).map(|v| song.archived = v).is_some(),
        SongField::Credits => read(value).map(|v| song.credits = v).is_some(),
        SongField::Key => read(value).map(|v| song.key = v).is_some(),
        SongField::Tags => read(value).map(|v| song.tags = v).is_some(),
        SongField::Background => read(value).map(|v| song.current.background = v).is_some(),
        SongField::Arrangement => read(value).map(|v| song.current.arrangement = v).is_some(),
    }
}
