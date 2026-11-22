use super::{Change, ItemField, ServiceField, ServiceOp, SongField, SongOp};
use crate::model::{Section, SectionKind};

impl Change {
    pub fn describe(&self, item_name: &dyn Fn(&str) -> String) -> String {
        match self {
            Change::Service { ops, .. } => service(ops, item_name),
            Change::Song { ops, .. } => song(ops),
        }
    }
}

fn service(ops: &[ServiceOp], name: &dyn Fn(&str) -> String) -> String {
    let ids: Vec<Option<&str>> = ops
        .iter()
        .map(|op| match op {
            ServiceOp::Set { .. } => None,
            ServiceOp::InsertItem { item, .. } | ServiceOp::RemoveItem { item, .. } => Some(item.id.as_str()),
            ServiceOp::MoveItem { id, .. } | ServiceOp::SetItem { id, .. } => Some(id.as_str()),
        })
        .collect();
    let count = ops.len();
    let all = |f: fn(&ServiceOp) -> bool| ops.iter().all(f);
    match ops {
        [] => "No change".into(),
        [ServiceOp::Set { field, .. }] => match field {
            ServiceField::Title => "Rename service",
            ServiceField::ScheduledAtMs => "Change date",
            ServiceField::Leader => "Change leader",
            ServiceField::Notes => "Edit service notes",
            ServiceField::EndedAtMs => "End service",
        }
        .into(),
        [ServiceOp::InsertItem { item, .. }] => format!("Add {}", name(&item.id)),
        [ServiceOp::RemoveItem { item, .. }] => format!("Remove {}", name(&item.id)),
        [ServiceOp::MoveItem { id, .. }] => format!("Move {}", name(id)),
        [ServiceOp::SetItem { id, field, .. }] => match field {
            ItemField::Title => format!("Rename {}", name(id)),
            ItemField::Key => format!("Change key of {}", name(id)),
            ItemField::Arrangement => format!("Change arrangement of {}", name(id)),
            ItemField::Background => format!("Change background of {}", name(id)),
            ItemField::Notes => format!("Edit notes of {}", name(id)),
            ItemField::Content | ItemField::VersionId => format!("Edit {}", name(id)),
        },
        _ if all(|op| matches!(op, ServiceOp::InsertItem { .. })) => format!("Add {count} items"),
        _ if all(|op| matches!(op, ServiceOp::RemoveItem { .. })) => format!("Remove {count} items"),
        _ if all(|op| matches!(op, ServiceOp::MoveItem { .. })) => "Reorder items".into(),
        _ if all(|op| matches!(op, ServiceOp::Set { .. })) => "Edit service details".into(),
        _ => match ids[0] {
            Some(id) if ids.iter().all(|i| *i == Some(id)) => format!("Edit {}", name(id)),
            _ => "Edit service".into(),
        },
    }
}

fn song(ops: &[SongOp]) -> String {
    match ops {
        [] => "No change".into(),
        [SongOp::Set { field, after, .. }] => match field {
            SongField::Title => "Rename song",
            SongField::Archived if after.as_bool() == Some(true) => "Archive song",
            SongField::Archived => "Restore song",
            SongField::Credits => "Edit credits",
            SongField::Key => "Change key",
            SongField::Tags => "Edit tags",
            SongField::Background => "Change background",
            SongField::Arrangement => "Change arrangement",
        }
        .into(),
        [SongOp::InsertSection { section, .. }, rest @ ..] if rest.iter().all(|op| matches!(op, SongOp::Set { field: SongField::Arrangement, .. })) => {
            format!("Add {}", section_label(section))
        }
        [SongOp::RemoveSection { section, .. }, rest @ ..] if rest.iter().all(|op| matches!(op, SongOp::Set { field: SongField::Arrangement, .. })) => {
            format!("Remove {}", section_label(section))
        }
        [SongOp::SetSection { after, .. }] => format!("Edit {}", section_label(after)),
        [SongOp::MoveSection { .. }, ..] if ops.iter().all(|op| matches!(op, SongOp::MoveSection { .. })) => "Reorder sections".into(),
        _ => "Edit song".into(),
    }
}

pub fn section_label(section: &Section) -> String {
    let name = section.name.as_deref().map(str::trim).filter(|n| !n.is_empty());
    let kind = match section.kind {
        SectionKind::Verse => "Verse",
        SectionKind::PreChorus => "Pre-chorus",
        SectionKind::Chorus => "Chorus",
        SectionKind::Bridge => "Bridge",
        SectionKind::Intro => "Intro",
        SectionKind::Outro => "Outro",
        SectionKind::Tag => "Tag",
        SectionKind::Other => return name.unwrap_or("Part").to_string(),
    };
    let base = match section.number {
        Some(n) => format!("{kind} {n}"),
        None => kind.to_string(),
    };
    match name {
        Some(name) => format!("{base} · {name}"),
        None => base,
    }
}
