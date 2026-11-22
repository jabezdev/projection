use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Section, Service, ServiceItem, Song};

mod label;
mod list;
mod service;
mod song;

#[cfg(test)]
mod tests;

pub use label::section_label;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EditStep {
    pub id: String,
    pub at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverts: Option<String>,
    pub change: Change,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    Service { id: String, ops: Vec<ServiceOp> },
    Song { id: String, ops: Vec<SongOp> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ServiceOp {
    Set { field: ServiceField, before: Value, after: Value },
    InsertItem { item: ServiceItem, prev: Option<String> },
    RemoveItem { item: ServiceItem, prev: Option<String> },
    MoveItem { id: String, from: Option<String>, to: Option<String> },
    SetItem { id: String, field: ItemField, before: Value, after: Value },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ServiceField {
    ScheduledAtMs,
    Title,
    Leader,
    Notes,
    EndedAtMs,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ItemField {
    Title,
    Notes,
    Background,
    Content,
    Key,
    Arrangement,
    VersionId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SongOp {
    Set { field: SongField, before: Value, after: Value },
    InsertSection { section: Section, prev: Option<String> },
    RemoveSection { section: Section, prev: Option<String> },
    MoveSection { id: String, from: Option<String>, to: Option<String> },
    SetSection { id: String, before: Section, after: Section },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum SongField {
    Title,
    Archived,
    Credits,
    Key,
    Tags,
    Background,
    Arrangement,
}

pub fn diff_service(before: &Service, after: &Service) -> Vec<ServiceOp> {
    service::diff(before, after)
}

pub fn diff_song(before: &Song, after: &Song) -> Vec<SongOp> {
    song::diff(before, after)
}

impl Change {
    pub fn record_id(&self) -> &str {
        match self {
            Change::Service { id, .. } | Change::Song { id, .. } => id,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Change::Service { ops, .. } => ops.is_empty(),
            Change::Song { ops, .. } => ops.is_empty(),
        }
    }

    pub fn inverted(&self) -> Change {
        match self {
            Change::Service { id, ops } => Change::Service { id: id.clone(), ops: ops.iter().rev().map(ServiceOp::inverted).collect() },
            Change::Song { id, ops } => Change::Song { id: id.clone(), ops: ops.iter().rev().map(SongOp::inverted).collect() },
        }
    }

    pub fn apply_to_service(&self, target: &mut Service) -> usize {
        match self {
            Change::Service { ops, .. } => ops.iter().filter(|op| !op.apply(target)).count(),
            Change::Song { ops, .. } => ops.len(),
        }
    }

    pub fn apply_to_song(&self, target: &mut Song) -> usize {
        match self {
            Change::Song { ops, .. } => ops.iter().filter(|op| !op.apply(target)).count(),
            Change::Service { ops, .. } => ops.len(),
        }
    }

    pub fn merge(&self, later: &Change) -> Option<Change> {
        match (self, later) {
            (Change::Service { id, ops }, Change::Service { id: later_id, ops: later_ops }) if id == later_id => {
                service::merge(ops, later_ops).map(|ops| Change::Service { id: id.clone(), ops })
            }
            (Change::Song { id, ops }, Change::Song { id: later_id, ops: later_ops }) if id == later_id => {
                song::merge(ops, later_ops).map(|ops| Change::Song { id: id.clone(), ops })
            }
            _ => None,
        }
    }
}
