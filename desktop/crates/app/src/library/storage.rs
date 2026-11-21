//! Saves projects on a worker thread, so a slow disk never delays a cue.
//!
//! The engine applies an edit as soon as it arrives; the worker writes saves
//! in arrival order and reports failures as engine errors. Loads wait for
//! queued saves, so a load never reads a project older than the last edit.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crossbeam::channel::{unbounded, Receiver, Sender};
use tokio::sync::broadcast;

use core_engine::ProjectStore;
use protocol::{EngineEvent, ProjectModel};

/// Where saves are written: the database in the app, a fake in tests.
pub trait ProjectWriter: Send + 'static {
    fn load(&self, project_id: i64) -> anyhow::Result<ProjectModel>;
    fn save(&self, project: &mut ProjectModel) -> anyhow::Result<()>;
}

#[derive(Default)]
struct Queued {
    count: Mutex<usize>,
    drained: Condvar,
}

#[derive(Clone)]
pub struct StorageWorker<W> {
    writer: Arc<W>,
    saves: Sender<ProjectModel>,
    queued: Arc<Queued>,
}

impl<W: ProjectWriter + Sync> StorageWorker<W> {
    pub fn start(writer: W, events: broadcast::Sender<EngineEvent>) -> anyhow::Result<Self> {
        let writer = Arc::new(writer);
        let queued = Arc::new(Queued::default());
        let (saves, received) = unbounded();
        {
            let (writer, queued) = (writer.clone(), queued.clone());
            std::thread::Builder::new()
                .name("storage".into())
                .spawn(move || run(&*writer, &received, &queued, &events))?;
        }
        Ok(Self { writer, saves, queued })
    }

    /// Waits until every queued save is written, up to `timeout`. Returns
    /// false if saves were still pending.
    pub fn flush(&self, timeout: Duration) -> bool {
        let count = self.queued.count.lock().expect("storage queue poisoned");
        let (count, _) = self
            .queued
            .drained
            .wait_timeout_while(count, timeout, |count| *count > 0)
            .expect("storage queue poisoned");
        *count == 0
    }
}

/// Loads may wait this long for earlier saves before reading anyway.
const LOAD_WAIT: Duration = Duration::from_secs(10);

impl<W: ProjectWriter + Sync> ProjectStore for StorageWorker<W> {
    fn load_project(&self, project_id: i64) -> anyhow::Result<ProjectModel> {
        if !self.flush(LOAD_WAIT) {
            log::warn!("Loading project {project_id} while earlier saves are still being written");
        }
        self.writer.load(project_id)
    }

    fn save_project(&self, project: &mut ProjectModel) -> anyhow::Result<()> {
        *self.queued.count.lock().expect("storage queue poisoned") += 1;
        if self.saves.send(project.clone()).is_err() {
            self.finished(1);
            anyhow::bail!("The storage worker has stopped");
        }
        Ok(())
    }
}

impl<W> StorageWorker<W> {
    fn finished(&self, saves: usize) {
        finish(&self.queued, saves);
    }
}

fn finish(queued: &Queued, saves: usize) {
    let mut count = queued.count.lock().expect("storage queue poisoned");
    *count = count.saturating_sub(saves);
    if *count == 0 {
        queued.drained.notify_all();
    }
}

fn run<W: ProjectWriter>(writer: &W, saves: &Receiver<ProjectModel>, queued: &Queued, events: &broadcast::Sender<EngineEvent>) {
    while let Ok(first) = saves.recv() {
        // A burst of edits to one project needs only its last state written.
        let mut batch = vec![first];
        batch.extend(saves.try_iter());
        let count = batch.len();
        let mut latest: Vec<ProjectModel> = Vec::new();
        for project in batch {
            match latest.last_mut() {
                Some(last) if last.id == project.id => *last = project,
                _ => latest.push(project),
            }
        }
        for mut project in latest {
            if let Err(e) = writer.save(&mut project) {
                log::error!("Saving project {}: {e:#}", project.id);
                let _ = events.send(EngineEvent::Error {
                    source: "project".into(),
                    message: format!("Failed to save project {}: {e:#}", project.id),
                    recoverable: true,
                });
            }
        }
        finish(queued, count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Default)]
    struct SlowDisk {
        written: Mutex<Vec<(i64, String)>>,
        blocked: AtomicBool,
        fail: AtomicBool,
    }

    impl ProjectWriter for Arc<SlowDisk> {
        fn load(&self, project_id: i64) -> anyhow::Result<ProjectModel> {
            let written = self.written.lock().unwrap();
            let name = written.iter().rev().find(|(id, _)| *id == project_id).map(|(_, name)| name.clone()).unwrap_or_default();
            Ok(ProjectModel { id: project_id, name, session: Default::default() })
        }

        fn save(&self, project: &mut ProjectModel) -> anyhow::Result<()> {
            while self.blocked.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
            anyhow::ensure!(!self.fail.load(Ordering::Acquire), "disk full");
            self.written.lock().unwrap().push((project.id, project.name.clone()));
            Ok(())
        }
    }

    fn project(id: i64, name: &str) -> ProjectModel {
        ProjectModel { id, name: name.into(), session: Default::default() }
    }

    #[test]
    fn saves_return_at_once_and_loads_see_every_earlier_save() {
        let disk = Arc::new(SlowDisk::default());
        disk.blocked.store(true, Ordering::Release);
        let (events, _) = broadcast::channel(8);
        let store = StorageWorker::start(disk.clone(), events).unwrap();
        for name in ["a", "b", "c"] {
            store.save_project(&mut project(1, name)).unwrap();
        }
        assert!(!store.flush(Duration::from_millis(50)), "the disk is still blocked");
        disk.blocked.store(false, Ordering::Release);
        assert_eq!(store.load_project(1).unwrap().name, "c");
        let written = disk.written.lock().unwrap();
        assert_eq!(written.last(), Some(&(1, "c".to_string())));
        assert!(written.len() <= 3, "bursts of saves of one project may merge");
    }

    #[test]
    fn saves_of_different_projects_are_all_written_in_order() {
        let disk = Arc::new(SlowDisk::default());
        disk.blocked.store(true, Ordering::Release);
        let (events, _) = broadcast::channel(8);
        let store = StorageWorker::start(disk.clone(), events).unwrap();
        store.save_project(&mut project(1, "first")).unwrap();
        store.save_project(&mut project(2, "second")).unwrap();
        store.save_project(&mut project(1, "third")).unwrap();
        disk.blocked.store(false, Ordering::Release);
        assert!(store.flush(Duration::from_secs(5)));
        let written = disk.written.lock().unwrap().clone();
        let names: Vec<_> = written.iter().map(|(_, name)| name.as_str()).collect();
        assert_eq!(names, ["first", "second", "third"], "only adjacent saves of one project merge");
    }

    #[test]
    fn a_failed_save_is_reported_as_an_engine_error() {
        let disk = Arc::new(SlowDisk::default());
        disk.fail.store(true, Ordering::Release);
        let (events, mut errors) = broadcast::channel(8);
        let store = StorageWorker::start(disk, events).unwrap();
        store.save_project(&mut project(4, "lost")).unwrap();
        assert!(store.flush(Duration::from_secs(5)));
        match errors.try_recv() {
            Ok(EngineEvent::Error { source, message, .. }) => {
                assert_eq!(source, "project");
                assert!(message.contains("disk full"), "{message}");
            }
            other => panic!("expected a save error, got {other:?}"),
        }
    }
}
