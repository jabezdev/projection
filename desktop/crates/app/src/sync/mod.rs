mod client;
mod commands;
mod credentials;
mod cycle;
mod live;
mod media;
mod outbox;

#[cfg(test)]
mod e2e_tests;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use core_engine::EngineCommand;
use protocol::sync::{SyncState, SyncStatus};
use tauri::{Emitter, Manager};
use tokio::sync::Notify;

pub use commands::*;
use client::Convex;
use credentials::Credentials;

const CYCLE_EVERY: Duration = Duration::from_secs(15);
const PUSH_AFTER: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(300);

pub struct SyncHandle(Arc<Shared>);

struct Shared {
    dir: PathBuf,
    credentials: Mutex<Credentials>,
    status: RwLock<SyncStatus>,
    keep_mine: Mutex<Vec<String>>,
    wake: Notify,
    forced: AtomicBool,
}

impl Shared {
    fn credentials(&self) -> Credentials {
        self.credentials.lock().map(|c| c.clone()).unwrap_or_default()
    }

    fn poke(&self) {
        self.forced.store(true, Ordering::Relaxed);
        self.wake.notify_one();
    }
}

pub fn start(app: &tauri::AppHandle, dir: PathBuf) -> std::io::Result<()> {
    let shared = Arc::new(Shared {
        credentials: Mutex::new(Credentials::load(&dir)),
        dir,
        status: RwLock::default(),
        keep_mine: Mutex::default(),
        wake: Notify::new(),
        forced: AtomicBool::new(true),
    });
    app.manage(SyncHandle(shared.clone()));
    let app = app.clone();
    std::thread::Builder::new().name("sync".into()).spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(e) => return log::error!("sync: could not start: {e}"),
        };
        runtime.block_on(run(app, shared));
    })?;
    Ok(())
}

async fn run(app: tauri::AppHandle, shared: Arc<Shared>) {
    let busy = Arc::new(AtomicBool::new(false));
    let mut live = live::Live::default();
    let mut last_cycle: Option<Instant> = None;
    let mut backoff = Duration::ZERO;
    let mut failed_at: Option<Instant> = None;
    let mut connection: Option<(String, Convex)> = None;
    let mut transfers: Option<tokio::task::JoinHandle<()>> = None;
    loop {
        tokio::select! {
            _ = shared.wake.notified() => {}
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
        let credentials = shared.credentials();
        let state = app.state::<crate::AppState>();
        let Some((url, token)) = credentials.paired() else {
            if let Some(task) = transfers.take() { task.abort(); }
            live.reset();
            report(&app, &shared, |s| *s = SyncStatus { state: SyncState::Off, ..SyncStatus::default() });
            continue;
        };
        if connection.as_ref().is_none_or(|(u, _)| u != url) {
            connection = Convex::new(url).ok().map(|c| (url.to_string(), c));
            if let Some(task) = transfers.take() { task.abort(); }
        }
        let Some((_, convex)) = connection.clone() else { continue };
        let snapshot = state.snapshot.read().map(|s| s.clone()).unwrap_or_default();
        busy.store(snapshot.playback.as_ref().is_some_and(|p| !p.paused), Ordering::Relaxed);
        if credentials.offline {
            live.reset();
            if let Some(task) = transfers.take() { task.abort(); }
            report(&app, &shared, |s| s.state = SyncState::Offline);
            continue;
        }
        if failed_at.is_some_and(|at| at.elapsed() < backoff) && !shared.forced.load(Ordering::Relaxed) {
            continue;
        }
        let today = start_of_today();
        let wanted = live::live_service(&snapshot, today);
        let live_result = async {
            live.claim(&convex, token, wanted.clone()).await?;
            live.show(&convex, token, live::now_showing(&snapshot)).await
        }
        .await;
        let pending = state.db.lock().ok().and_then(|db| db.pending_count().ok()).unwrap_or(0);
        let forced = shared.forced.swap(false, Ordering::Relaxed);
        let due = forced || last_cycle.is_none_or(|at| at.elapsed() >= CYCLE_EVERY) || (pending > 0 && last_cycle.is_none_or(|at| at.elapsed() >= PUSH_AFTER));
        if !due {
            if let Err(e) = live_result { failed(&app, &shared, &e, &mut failed_at, &mut backoff); }
            continue;
        }
        let keep_mine = take_resolutions(&shared, &state.db);
        report(&app, &shared, |s| s.state = SyncState::Syncing);
        let result = match live_result {
            Ok(()) => cycle::run(&convex, token, &state.db, keep_mine).await,
            Err(e) => Err(e),
        };
        last_cycle = Some(Instant::now());
        match result {
            Ok(outcome) => {
                backoff = Duration::ZERO;
                failed_at = None;
                if !outcome.changed.is_empty() {
                    reload_loaded(&state, &outcome.changed);
                    let _ = app.emit("library:changed", outcome.changed.iter().collect::<Vec<_>>());
                }
                if transfers.as_ref().is_none_or(|t| t.is_finished()) {
                    let worker = media::Transfers { convex: convex.clone(), token: token.to_string(), db: state.db.clone(), dir: shared.dir.clone(), busy: busy.clone() };
                    let app = app.clone();
                    transfers = Some(tokio::spawn(async move { move_media(app, worker).await }));
                }
                let (pending, conflicts, last) = counts(&state.db);
                report(&app, &shared, |s| {
                    s.state = SyncState::Idle;
                    s.error = None;
                    s.pending = pending;
                    s.conflicts = conflicts;
                    s.last_synced_ms = last;
                    s.live_service = live.claimed().map(str::to_string);
                    s.error = live.held_by.as_ref().map(|who| format!("{who} is running this service live, so changes here wait"));
                });
            }
            Err(e) => failed(&app, &shared, &e, &mut failed_at, &mut backoff),
        }
    }
}

async fn move_media(app: tauri::AppHandle, worker: media::Transfers) {
    loop {
        let queue = worker.queue().unwrap_or_default();
        if let Some(shared) = app.try_state::<SyncHandle>() {
            report(&app, &shared.0, |s| { s.uploads = queue.uploads; s.downloads = queue.downloads; });
        }
        match worker.step().await {
            Ok(true) => {
                let _ = app.emit("library:changed", Vec::<String>::new());
            }
            Ok(false) => tokio::time::sleep(Duration::from_secs(5)).await,
            Err(e) => {
                log::warn!("sync: media transfer failed: {e:#}");
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
    }
}

fn failed(app: &tauri::AppHandle, shared: &Shared, error: &anyhow::Error, failed_at: &mut Option<Instant>, backoff: &mut Duration) {
    *failed_at = Some(Instant::now());
    *backoff = (*backoff * 2).clamp(Duration::from_secs(10), MAX_BACKOFF);
    let refused = client::is_refusal(error);
    log::warn!("sync: {error:#}");
    report(app, shared, |s| {
        s.state = if refused { SyncState::Error } else { SyncState::Offline };
        s.error = Some(if refused { error.to_string() } else { "Can't reach the team library. Everything keeps working here, and changes go up when the connection is back.".into() });
    });
}

fn take_resolutions(shared: &Shared, db: &crate::Db) -> Vec<(String, protocol::sync::Push)> {
    let ids = shared.keep_mine.lock().map(|mut ids| std::mem::take(&mut *ids)).unwrap_or_default();
    let Ok(db) = db.lock() else { return vec![] };
    ids.into_iter().filter_map(|id| db.conflict(&id).ok().flatten().map(|c| (id, c.keep_mine))).collect()
}

fn counts(db: &crate::Db) -> (usize, usize, Option<i64>) {
    let Ok(db) = db.lock() else { return (0, 0, None) };
    let last = db.meta("sync_last_ms").ok().flatten().and_then(|v| v.parse().ok());
    (db.pending_count().unwrap_or(0), db.conflicts().map(|c| c.len()).unwrap_or(0), last)
}

fn report(app: &tauri::AppHandle, shared: &Shared, change: impl FnOnce(&mut SyncStatus)) {
    let Ok(mut status) = shared.status.write() else { return };
    let before = status.clone();
    change(&mut status);
    let credentials = shared.credentials();
    status.paired = credentials.paired().is_some();
    status.deployment = credentials.deployment.clone();
    status.device_name = credentials.device_name.clone();
    status.offline = credentials.offline;
    if *status != before {
        let _ = app.emit("sync:status", status.clone());
    }
}

fn reload_loaded(state: &crate::AppState, changed: &std::collections::HashSet<String>) {
    let loaded = state.snapshot.read().ok().and_then(|s| s.service.as_ref().map(|l| l.service.id.clone()));
    let Some(id) = loaded.filter(|id| changed.contains(id)) else { return };
    let service = state.db.lock().ok().and_then(|db| db.service(&id).ok().flatten());
    if let Some(service) = service {
        let _ = state.send(EngineCommand::SaveService { service });
    }
}

fn start_of_today() -> i64 {
    use chrono::{Local, TimeZone};
    let today = Local::now().date_naive().and_hms_opt(0, 0, 0).expect("midnight");
    Local.from_local_datetime(&today).earliest().map(|d| d.timestamp_millis()).unwrap_or(0)
}
