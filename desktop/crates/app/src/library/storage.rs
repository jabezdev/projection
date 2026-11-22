use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crossbeam::channel::{unbounded, Receiver, Sender};
use tokio::sync::broadcast;

use core_engine::ServiceStore;
use protocol::model::Service;
use protocol::{EngineEvent, LiveService};

pub trait ServiceWriter: Send + 'static {
    fn load(&self, service_id: &str) -> anyhow::Result<Service>;
    fn save(&self, service: &Service) -> anyhow::Result<()>;
    fn resolve(&self, service: &Service) -> anyhow::Result<LiveService>;
    fn step(&self, service_id: &str, redo: bool) -> anyhow::Result<Option<Service>>;
}

pub type StepReply = Result<Option<Service>, String>;

#[derive(Default)]
struct Replies {
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, Sender<StepReply>>>,
}

#[derive(Default)]
struct Queued {
    count: Mutex<usize>,
    drained: Condvar,
}

#[derive(Clone)]
pub struct StorageWorker<W> {
    writer: Arc<W>,
    saves: Sender<Service>,
    queued: Arc<Queued>,
    replies: Arc<Replies>,
}

impl<W: ServiceWriter + Sync> StorageWorker<W> {
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
        Ok(Self { writer, saves, queued, replies: Arc::default() })
    }

    pub fn expect_step(&self) -> (u64, Receiver<StepReply>) {
        let request = self.replies.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (sender, receiver) = crossbeam::channel::bounded(1);
        self.replies.waiting.lock().expect("replies poisoned").insert(request, sender);
        (request, receiver)
    }

    pub fn forget_step(&self, request: u64) {
        self.replies.waiting.lock().expect("replies poisoned").remove(&request);
    }

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

const LOAD_WAIT: Duration = Duration::from_secs(10);

impl<W: ServiceWriter + Sync> ServiceStore for StorageWorker<W> {
    fn load_service(&self, service_id: &str) -> anyhow::Result<LiveService> {
        if !self.flush(LOAD_WAIT) {
            log::warn!("Loading service {service_id} while earlier saves are still being written");
        }
        let service = self.writer.load(service_id)?;
        self.writer.resolve(&service)
    }

    fn save_service(&self, service: &Service) -> anyhow::Result<LiveService> {
        *self.queued.count.lock().expect("storage queue poisoned") += 1;
        if self.saves.send(service.clone()).is_err() {
            self.finished(1);
            anyhow::bail!("The storage worker has stopped");
        }
        self.writer.resolve(service)
    }

    fn resolve_service(&self, service: &Service) -> anyhow::Result<LiveService> {
        self.writer.resolve(service)
    }

    fn step_service(&self, service_id: &str, redo: bool, request: u64) -> anyhow::Result<Option<LiveService>> {
        let stepped = if self.flush(LOAD_WAIT) { self.writer.step(service_id, redo) } else { Err(anyhow::anyhow!("earlier saves are still being written")) };
        if let Some(reply) = self.replies.waiting.lock().expect("replies poisoned").remove(&request) {
            let _ = reply.send(stepped.as_ref().map(Clone::clone).map_err(|e| format!("{e:#}")));
        }
        stepped?.map(|service| self.writer.resolve(&service)).transpose()
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

fn run<W: ServiceWriter>(writer: &W, saves: &Receiver<Service>, queued: &Queued, events: &broadcast::Sender<EngineEvent>) {
    while let Ok(first) = saves.recv() {
        let mut batch = vec![first];
        batch.extend(saves.try_iter());
        let count = batch.len();
        let mut latest: Vec<Service> = Vec::new();
        for service in batch {
            match latest.last_mut() {
                Some(last) if last.id == service.id => *last = service,
                _ => latest.push(service),
            }
        }
        for service in latest {
            if let Err(e) = writer.save(&service) {
                log::error!("Saving service {}: {e:#}", service.id);
                let _ = events.send(EngineEvent::Error {
                    source: "service".into(),
                    message: format!("Failed to save service {}: {e:#}", service.id),
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
        written: Mutex<Vec<(String, String)>>,
        blocked: AtomicBool,
        fail: AtomicBool,
    }

    impl ServiceWriter for Arc<SlowDisk> {
        fn load(&self, service_id: &str) -> anyhow::Result<Service> {
            let written = self.written.lock().unwrap();
            let title = written.iter().rev().find(|(id, _)| id == service_id).map(|(_, title)| title.clone()).unwrap_or_default();
            Ok(service(service_id, &title))
        }

        fn save(&self, service: &Service) -> anyhow::Result<()> {
            while self.blocked.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
            anyhow::ensure!(!self.fail.load(Ordering::Acquire), "disk full");
            self.written.lock().unwrap().push((service.id.clone(), service.title.clone()));
            Ok(())
        }

        fn resolve(&self, service: &Service) -> anyhow::Result<LiveService> {
            Ok(LiveService { service: service.clone(), items: Vec::new() })
        }

        fn step(&self, service_id: &str, _redo: bool) -> anyhow::Result<Option<Service>> {
            let written = self.written.lock().unwrap();
            Ok(written.iter().rev().find(|(id, _)| id == service_id).map(|(id, title)| service(id, &format!("{title} undone"))))
        }
    }

    fn service(id: &str, title: &str) -> Service {
        Service { id: id.into(), scheduled_at_ms: 0, title: title.into(), leader: None, notes: None, ended_at_ms: None, items: Vec::new() }
    }

    #[test]
    fn saves_return_at_once_and_loads_see_every_earlier_save() {
        let disk = Arc::new(SlowDisk::default());
        disk.blocked.store(true, Ordering::Release);
        let (events, _) = broadcast::channel(8);
        let store = StorageWorker::start(disk.clone(), events).unwrap();
        for name in ["a", "b", "c"] {
            store.save_service(&service("1", name)).unwrap();
        }
        assert!(!store.flush(Duration::from_millis(50)), "the disk is still blocked");
        disk.blocked.store(false, Ordering::Release);
        assert_eq!(store.load_service("1").unwrap().service.title, "c");
        let written = disk.written.lock().unwrap();
        assert_eq!(written.last(), Some(&("1".to_string(), "c".to_string())));
        assert!(written.len() <= 3, "bursts of saves of one service may merge");
    }

    #[test]
    fn saves_of_different_services_are_all_written_in_order() {
        let disk = Arc::new(SlowDisk::default());
        disk.blocked.store(true, Ordering::Release);
        let (events, _) = broadcast::channel(8);
        let store = StorageWorker::start(disk.clone(), events).unwrap();
        store.save_service(&service("1", "first")).unwrap();
        store.save_service(&service("2", "second")).unwrap();
        store.save_service(&service("1", "third")).unwrap();
        disk.blocked.store(false, Ordering::Release);
        assert!(store.flush(Duration::from_secs(5)));
        let written = disk.written.lock().unwrap().clone();
        let names: Vec<_> = written.iter().map(|(_, name)| name.as_str()).collect();
        assert_eq!(names, ["first", "second", "third"], "only adjacent saves of one service merge");
    }

    #[test]
    fn undo_waits_for_earlier_saves_and_answers_the_waiting_command() {
        let disk = Arc::new(SlowDisk::default());
        disk.blocked.store(true, Ordering::Release);
        let (events, _) = broadcast::channel(8);
        let store = StorageWorker::start(disk.clone(), events).unwrap();
        store.save_service(&service("1", "first")).unwrap();
        store.save_service(&service("2", "other")).unwrap();
        store.save_service(&service("1", "second")).unwrap();
        let (request, reply) = store.expect_step();
        let unblock = {
            let disk = disk.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(30));
                disk.blocked.store(false, Ordering::Release);
            })
        };
        let live = store.step_service("1", false, request).unwrap();
        unblock.join().unwrap();
        assert_eq!(live.map(|l| l.service.title).as_deref(), Some("second undone"), "the step saw every earlier save");
        assert_eq!(reply.try_recv().unwrap().unwrap().map(|s| s.title).as_deref(), Some("second undone"));
    }

    #[test]
    fn a_failed_save_is_reported_as_an_engine_error() {
        let disk = Arc::new(SlowDisk::default());
        disk.fail.store(true, Ordering::Release);
        let (events, mut errors) = broadcast::channel(8);
        let store = StorageWorker::start(disk, events).unwrap();
        store.save_service(&service("4", "lost")).unwrap();
        assert!(store.flush(Duration::from_secs(5)));
        match errors.try_recv() {
            Ok(EngineEvent::Error { source, message, .. }) => {
                assert_eq!(source, "service");
                assert!(message.contains("disk full"), "{message}");
            }
            other => panic!("expected a save error, got {other:?}"),
        }
    }
}
