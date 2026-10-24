//! Screen and window capture (roadmap phase 8). A capture item's source is
//! captured once while it is on air and shown on the screens it is routed
//! to, like a video: frames go to the renderer's shared texture as BGRA and
//! are scaled there. The method is chosen per platform (detected, with a
//! default and a setting to switch): Windows.Graphics.Capture or Desktop
//! Duplication on Windows; the PipeWire screen-cast portal or X11 on Linux.
//!
//! A source that disappears (window closed, display unplugged, sharing
//! stopped) is replaced by black at once, never left frozen on air, and the
//! operator is told.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use protocol::{CaptureKind, CaptureSource};
use render_engine::{FramePixels, PixelFrame};
use serde::Serialize;

#[cfg(target_os = "linux")]
mod portal;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
mod x11;
mod test_pattern;

/// Frames reach the outputs at most this often; screens rarely need more.
const MIN_FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// One captured frame, BGRA (or BGRx) rows.
pub struct CaptureFrame {
    id: u64,
    width: u32,
    height: u32,
    stride: usize,
    data: Vec<u8>,
}

impl PixelFrame for CaptureFrame {
    fn id(&self) -> u64 {
        self.id
    }
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    fn pixels(&self) -> FramePixels<'_> {
        FramePixels::Bgra { data: &self.data, stride: self.stride }
    }
}

fn next_frame_id() -> u64 {
    // Far from decoder ids, which count up from 1.
    static NEXT: AtomicU64 = AtomicU64::new(1 << 62);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[derive(Default)]
struct Slot {
    latest: Option<Arc<CaptureFrame>>,
    shown: Option<(Arc<CaptureFrame>, Instant)>,
    lost: Option<String>,
}

/// Where a running capture puts its frames; the renderer takes the newest.
#[derive(Clone, Default)]
pub struct FrameSlot(Arc<Mutex<Slot>>);

impl FrameSlot {
    /// Publishes a frame of `width`×`height` BGRA pixels with `stride` bytes per row.
    pub fn publish(&self, width: u32, height: u32, stride: usize, data: Vec<u8>) {
        if width == 0 || height == 0 || data.len() < stride * (height as usize - 1) + width as usize * 4 {
            return;
        }
        let frame = Arc::new(CaptureFrame { id: next_frame_id(), width, height, stride, data });
        self.0.lock().expect("capture slot poisoned").latest = Some(frame);
    }

    /// The source is gone: show black (in its last size) instead of its last picture.
    pub fn lose(&self, reason: String) {
        let mut slot = self.0.lock().expect("capture slot poisoned");
        if slot.lost.is_some() {
            return;
        }
        let (width, height) = slot.latest.as_ref().map_or((1280, 720), |f| (f.width, f.height));
        log::warn!("Capture lost: {reason}");
        slot.latest = Some(Arc::new(CaptureFrame { id: next_frame_id(), width, height, stride: width as usize * 4, data: vec![0; width as usize * height as usize * 4] }));
        slot.shown = None;
        slot.lost = Some(reason);
    }

    pub fn lost(&self) -> Option<String> {
        self.0.lock().expect("capture slot poisoned").lost.clone()
    }

    /// The frame to show now: the newest, but no more often than
    /// [`MIN_FRAME_INTERVAL`] (the previous one meanwhile), so nothing newer
    /// is ever held back for long.
    fn frame(&self, now: Instant) -> Option<Arc<CaptureFrame>> {
        let mut slot = self.0.lock().expect("capture slot poisoned");
        let latest = slot.latest.clone()?;
        match &slot.shown {
            Some((shown, at)) if shown.id != latest.id && now.duration_since(*at) < MIN_FRAME_INTERVAL => Some(shown.clone()),
            Some((shown, _)) if shown.id == latest.id => Some(latest),
            _ => {
                slot.shown = Some((latest.clone(), now));
                Some(latest)
            }
        }
    }
}

/// A running capture; dropping it stops capturing.
pub trait CaptureStream: Send {}

/// One capture method.
pub trait Backend: Send + Sync {
    /// `wgc`, `dxgi`, `portal` or `x11`.
    fn name(&self) -> &'static str;
    fn label(&self) -> &'static str;
    /// The system shows its own picker (the portal) instead of a list.
    fn system_picker(&self) -> bool {
        false
    }
    /// Screens and windows to choose from.
    fn sources(&self) -> anyhow::Result<Vec<CaptureSource>>;
    /// Asks the system to pick a source (only with [`Backend::system_picker`]).
    fn pick(&self, _kind: CaptureKind) -> anyhow::Result<Option<CaptureSource>> {
        anyhow::bail!("{} lists its sources instead", self.label())
    }
    fn start(&self, source: &CaptureSource, slot: FrameSlot) -> anyhow::Result<Box<dyn CaptureStream>>;
}

/// Capture methods that work here, best first.
#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
fn detect(data_dir: Option<&std::path::Path>) -> Vec<Arc<dyn Backend>> {
    let mut backends: Vec<Arc<dyn Backend>> = Vec::new();
    // Test aid: a moving pattern instead of a real screen.
    if std::env::var("MAGI_TEST_CAPTURE").as_deref() == Ok("1") {
        backends.push(Arc::new(test_pattern::TestPattern));
    }
    #[cfg(windows)]
    {
        if windows::wgc_supported() {
            backends.push(Arc::new(windows::Wgc));
        }
        backends.push(Arc::new(windows::Dxgi));
    }
    #[cfg(target_os = "linux")]
    {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland");
        if wayland {
            backends.push(Arc::new(portal::Portal::new(data_dir)));
        }
        if std::env::var_os("DISPLAY").is_some() {
            backends.push(Arc::new(x11::X11));
        }
        if !wayland {
            backends.push(Arc::new(portal::Portal::new(data_dir)));
        }
    }
    backends
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CaptureStatus {
    pub id: String,
    pub name: String,
    pub error: Option<String>,
}

pub struct Captures {
    backends: Vec<Arc<dyn Backend>>,
    /// The chosen method's name; empty for the default (the first).
    choice: Mutex<String>,
    running: Mutex<HashMap<String, (CaptureSource, Box<dyn CaptureStream>, FrameSlot)>>,
    /// Sources that could not start, by id, with the reason; they show black.
    failed: Mutex<HashMap<String, (CaptureSource, String, FrameSlot)>>,
}

impl Captures {
    /// `data_dir` keeps what a method needs to reopen sources (portal tokens).
    pub fn new(choice: &str, data_dir: &std::path::Path) -> Self {
        Self::with_backends(detect(Some(data_dir)), choice)
    }

    fn with_backends(backends: Vec<Arc<dyn Backend>>, choice: &str) -> Self {
        Self { backends, choice: Mutex::new(choice.to_string()), running: Mutex::default(), failed: Mutex::default() }
    }

    pub fn backend(&self) -> Option<Arc<dyn Backend>> {
        let choice = self.choice.lock().expect("capture choice poisoned").clone();
        self.backends.iter().find(|b| b.name() == choice).or(self.backends.first()).cloned()
    }

    /// Switches method; running captures restart with it.
    pub fn set_choice(&self, choice: &str) {
        *self.choice.lock().expect("capture choice poisoned") = choice.to_string();
        let sources: Vec<CaptureSource> = self.running.lock().expect("captures poisoned").values().map(|(s, ..)| s.clone()).collect();
        self.running.lock().expect("captures poisoned").clear();
        self.failed.lock().expect("captures poisoned").clear();
        self.set_active(&sources);
    }

    /// Keeps exactly these sources captured. Starting is quick for listed
    /// sources; a failure is reported in [`Captures::status`] and shows black.
    pub fn set_active(&self, sources: &[CaptureSource]) {
        let mut running = self.running.lock().expect("captures poisoned");
        let mut failed = self.failed.lock().expect("captures poisoned");
        running.retain(|id, _| sources.iter().any(|s| s.id == *id));
        failed.retain(|id, _| sources.iter().any(|s| s.id == *id));
        for source in sources {
            if running.contains_key(&source.id) || failed.contains_key(&source.id) {
                continue;
            }
            let slot = FrameSlot::default();
            let started = match self.backend() {
                Some(backend) => backend.start(source, slot.clone()),
                None => Err(anyhow::anyhow!("No screen capture method is available on this computer")),
            };
            match started {
                Ok(stream) => {
                    log::info!("Capturing {} ({})", source.name, source.id);
                    running.insert(source.id.clone(), (source.clone(), stream, slot));
                }
                Err(e) => {
                    log::error!("Cannot capture {}: {e:#}", source.name);
                    let reason = format!("{e:#}");
                    slot.lose(reason.clone());
                    failed.insert(source.id.clone(), (source.clone(), reason, slot));
                }
            }
        }
    }

    /// The frame to show for capture `id`; black when it failed to start.
    pub fn frame(&self, id: &str) -> Option<Arc<dyn PixelFrame>> {
        let slot = self.running.lock().expect("captures poisoned").get(id).map(|(_, _, slot)| slot.clone());
        let slot = slot.or_else(|| self.failed.lock().expect("captures poisoned").get(id).map(|(.., slot)| slot.clone()));
        match slot {
            Some(slot) => slot.frame(Instant::now()).map(|f| f as Arc<dyn PixelFrame>),
            None => None,
        }
    }

    /// Running and failed captures, for the operator.
    pub fn status(&self) -> Vec<CaptureStatus> {
        let running = self.running.lock().expect("captures poisoned");
        let failed = self.failed.lock().expect("captures poisoned");
        running
            .values()
            .map(|(source, _, slot)| CaptureStatus { id: source.id.clone(), name: source.name.clone(), error: slot.lost() })
            .chain(failed.values().map(|(source, error, _)| CaptureStatus { id: source.id.clone(), name: source.name.clone(), error: Some(error.clone()) }))
            .collect()
    }

    pub fn shutdown(&self) {
        self.running.lock().expect("captures poisoned").clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    struct Running;
    impl CaptureStream for Running {}
    impl Backend for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn label(&self) -> &'static str {
            "Fake"
        }
        fn sources(&self) -> anyhow::Result<Vec<CaptureSource>> {
            Ok(vec![])
        }
        fn start(&self, source: &CaptureSource, slot: FrameSlot) -> anyhow::Result<Box<dyn CaptureStream>> {
            anyhow::ensure!(source.id != "missing", "not found");
            slot.publish(2, 2, 8, vec![255; 16]);
            Ok(Box::new(Running))
        }
    }

    fn source(id: &str) -> CaptureSource {
        CaptureSource { kind: CaptureKind::Screen, id: id.into(), name: id.into() }
    }

    #[test]
    fn captures_follow_the_active_sources_and_failures_are_reported() {
        let captures = Captures::with_backends(vec![Arc::new(Fake)], "");
        captures.set_active(&[source("a"), source("missing")]);
        assert!(captures.frame("a").is_some());
        let status = captures.status();
        assert_eq!(status.iter().find(|s| s.id == "missing").unwrap().error.as_deref(), Some("not found"));
        let black = captures.frame("missing").expect("a source that cannot start shows black");
        assert!(matches!(black.pixels(), FramePixels::Bgra { data, .. } if data.iter().all(|&b| b == 0)));
        captures.set_active(&[]);
        assert!(captures.frame("a").is_none() && captures.status().is_empty(), "stopped when no longer on air");
    }

    #[test]
    fn a_lost_source_turns_black_instead_of_freezing() {
        let slot = FrameSlot::default();
        slot.publish(4, 2, 16, vec![200; 32]);
        let first = slot.frame(Instant::now()).unwrap();
        slot.lose("window closed".into());
        let after = slot.frame(Instant::now()).unwrap();
        assert_ne!(after.id, first.id);
        assert_eq!((after.width, after.height), (4, 2), "black in the same size");
        assert!(after.data.iter().all(|&b| b == 0));
        assert_eq!(slot.lost().as_deref(), Some("window closed"));
    }

    #[test]
    fn frames_are_capped_but_the_newest_always_arrives() {
        let slot = FrameSlot::default();
        let start = Instant::now();
        slot.publish(2, 2, 8, vec![1; 16]);
        let a = slot.frame(start).unwrap().id;
        slot.publish(2, 2, 8, vec![2; 16]);
        assert_eq!(slot.frame(start + Duration::from_millis(10)).unwrap().id, a, "too soon: the previous frame stays");
        assert_ne!(slot.frame(start + Duration::from_millis(40)).unwrap().id, a, "then the newest");
    }
}

#[derive(Serialize)]
pub struct CaptureMethod {
    name: &'static str,
    label: &'static str,
    system_picker: bool,
}

#[derive(Serialize)]
pub struct CaptureMethods {
    /// The method in use.
    current: Option<&'static str>,
    /// The saved choice; empty for automatic.
    choice: String,
    methods: Vec<CaptureMethod>,
}

#[tauri::command]
pub fn get_capture_methods(captures: tauri::State<'_, Arc<Captures>>) -> CaptureMethods {
    CaptureMethods {
        current: captures.backend().map(|b| b.name()),
        choice: captures.choice.lock().expect("capture choice poisoned").clone(),
        methods: captures.backends.iter().map(|b| CaptureMethod { name: b.name(), label: b.label(), system_picker: b.system_picker() }).collect(),
    }
}

/// Saves the capture method (empty for automatic); captures restart with it.
#[tauri::command]
pub fn set_capture_method(captures: tauri::State<'_, Arc<Captures>>, config: tauri::State<'_, crate::settings::ConfigStore>, name: String) -> Result<(), String> {
    if !name.is_empty() && !captures.backends.iter().any(|b| b.name() == name) {
        return Err(format!("Unknown capture method {name}"));
    }
    config.update(|c| c.capture_method = name.clone())?;
    captures.set_choice(&name);
    Ok(())
}

/// Screens and windows the current method can capture.
#[tauri::command]
pub async fn list_capture_sources(captures: tauri::State<'_, Arc<Captures>>) -> Result<Vec<CaptureSource>, String> {
    let backend = captures.backend().ok_or("No screen capture method is available on this computer")?;
    tauri::async_runtime::spawn_blocking(move || backend.sources().map_err(|e| format!("Listing screens and windows: {e:#}")))
        .await
        .map_err(|e| e.to_string())?
}

/// Lets the system dialog pick a screen or window (portal methods); `None` when cancelled.
#[tauri::command]
pub async fn pick_capture_source(captures: tauri::State<'_, Arc<Captures>>, kind: CaptureKind) -> Result<Option<CaptureSource>, String> {
    let backend = captures.backend().ok_or("No screen capture method is available on this computer")?;
    tauri::async_runtime::spawn_blocking(move || backend.pick(kind).map_err(|e| format!("{e:#}"))).await.map_err(|e| e.to_string())?
}

/// Captures running for the current cue, with any problem.
#[tauri::command]
pub fn get_capture_status(captures: tauri::State<'_, Arc<Captures>>) -> Vec<CaptureStatus> {
    captures.status()
}
