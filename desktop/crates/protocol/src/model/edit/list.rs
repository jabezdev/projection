use std::collections::{HashMap, HashSet};

use super::{Section, ServiceItem};

pub(super) trait Keyed: Clone + PartialEq {
    fn key(&self) -> &str;
}

impl Keyed for ServiceItem {
    fn key(&self) -> &str {
        &self.id
    }
}

impl Keyed for Section {
    fn key(&self) -> &str {
        &self.id
    }
}

pub(super) enum ListChange<T> {
    Insert(T, Option<String>),
    Remove(T, Option<String>),
    Move(String, Option<String>, Option<String>),
}

pub(super) fn diff<T: Keyed>(before: &[T], after: &[T]) -> Vec<ListChange<T>> {
    let mut work = before.to_vec();
    let mut changes = Vec::new();
    let wanted: HashSet<&str> = after.iter().map(Keyed::key).collect();
    let mut i = 0;
    while i < work.len() {
        if wanted.contains(work[i].key()) {
            i += 1;
            continue;
        }
        let prev = i.checked_sub(1).map(|p| work[p].key().to_string());
        changes.push(ListChange::Remove(work.remove(i), prev));
    }
    let positions: HashMap<String, usize> = work.iter().enumerate().map(|(i, v)| (v.key().to_string(), i)).collect();
    let kept = stable(after.iter().filter_map(|v| positions.get(v.key()).map(|&p| (v.key(), p))).collect());
    for (i, value) in after.iter().enumerate() {
        let to = i.checked_sub(1).map(|p| after[p].key().to_string());
        if !positions.contains_key(value.key()) {
            insert(&mut work, value.clone(), to.as_deref());
            changes.push(ListChange::Insert(value.clone(), to));
        } else if !kept.contains(value.key()) {
            let from = predecessor(&work, value.key());
            move_to(&mut work, value.key(), to.as_deref());
            changes.push(ListChange::Move(value.key().to_string(), from, to));
        }
    }
    changes
}

fn stable<'a>(sequence: Vec<(&'a str, usize)>) -> HashSet<&'a str> {
    let mut tails: Vec<usize> = Vec::new();
    let mut links: Vec<Option<usize>> = vec![None; sequence.len()];
    for (i, &(_, value)) in sequence.iter().enumerate() {
        let at = tails.partition_point(|&t| sequence[t].1 < value);
        links[i] = at.checked_sub(1).map(|p| tails[p]);
        if at == tails.len() {
            tails.push(i);
        } else {
            tails[at] = i;
        }
    }
    let mut kept = HashSet::new();
    let mut cursor = tails.last().copied();
    while let Some(i) = cursor {
        kept.insert(sequence[i].0);
        cursor = links[i];
    }
    kept
}

pub(super) fn index<T: Keyed>(list: &[T], key: &str) -> Option<usize> {
    list.iter().position(|v| v.key() == key)
}

pub(super) fn predecessor<T: Keyed>(list: &[T], key: &str) -> Option<String> {
    index(list, key).and_then(|i| i.checked_sub(1)).map(|p| list[p].key().to_string())
}

pub(super) fn insert<T: Keyed>(list: &mut Vec<T>, value: T, prev: Option<&str>) -> bool {
    if index(list, value.key()).is_some() {
        return false;
    }
    let (at, clean) = match prev {
        None => (0, true),
        Some(prev) => index(list, prev).map(|p| (p + 1, true)).unwrap_or((list.len(), false)),
    };
    list.insert(at, value);
    clean
}

pub(super) fn remove<T: Keyed>(list: &mut Vec<T>, value: &T) -> bool {
    let Some(i) = index(list, value.key()) else { return false };
    let removed = list.remove(i);
    removed == *value
}

pub(super) fn move_to<T: Keyed>(list: &mut Vec<T>, key: &str, to: Option<&str>) -> bool {
    let Some(i) = index(list, key) else { return false };
    let value = list.remove(i);
    insert(list, value, to)
}
