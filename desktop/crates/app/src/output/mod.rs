pub mod display_names;
pub mod displays;
pub mod media;
pub mod monitor_window;
pub mod scene;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use render_engine::{MonitorSpec, OutputRole, OutputSpec, PreviewTarget, RenderEngine, Scene, Snapshot, TestPatternSource, VideoFrames};
use tauri::{Emitter, Manager, Runtime, Window};

pub fn prepare_platform() {
    #[cfg(target_os = "linux")]
    if std::env::var("GDK_BACKEND").as_deref() != Ok("x11") {
        log::warn!("Native outputs require XWayland on Linux; setting GDK_BACKEND=x11");
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

pub struct NativeOutputs(Mutex<Option<RenderEngine>>);

impl NativeOutputs {
    pub fn resize(&self, role: OutputRole, width: u32, height: u32) {
        if let Ok(guard) = self.0.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.resize(role, width, height);
            }
        }
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn redraw(&self, role: OutputRole) {
        if let Ok(guard) = self.0.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.redraw(role);
            }
        }
    }

    pub fn set_shown(&self, role: OutputRole, shown: bool) {
        if let Ok(guard) = self.0.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.set_shown(role, shown);
            }
        }
    }

    pub fn set_aspect(&self, role: OutputRole, aspect: Option<(u32, u32)>) {
        if let Ok(guard) = self.0.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.set_aspect(role, aspect);
            }
        }
    }

    pub fn set_scene(&self, role: OutputRole, scene: Scene) {
        if let Ok(guard) = self.0.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.set_scene(role, scene);
            }
        }
    }

    pub fn fonts(&self) -> Option<Arc<render_engine::Fonts>> {
        self.0.lock().ok()?.as_ref().map(|engine| engine.fonts())
    }

    pub fn preload(&self, images: &[String]) {
        if let Ok(guard) = self.0.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.preload(images);
            }
        }
    }

    pub fn preview_style(&self, scene: Scene) {
        if let Ok(guard) = self.0.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.preview_style(scene);
            }
        }
    }

    pub fn add_monitor(&self, spec: MonitorSpec) -> anyhow::Result<()> {
        let guard = self.0.lock().map_err(|_| anyhow::anyhow!("native outputs poisoned"))?;
        guard.as_ref().ok_or_else(|| anyhow::anyhow!("Native outputs have shut down"))?.add_monitor(spec)
    }

    pub fn set_monitor(&self, role: OutputRole, size: Option<(u32, u32)>) {
        if let Ok(guard) = self.0.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.set_monitor(role, size);
            }
        }
    }

    pub fn shutdown(&self) {
        let engine = self.0.lock().ok().and_then(|mut guard| guard.take());
        if let Some(mut engine) = engine {
            engine.shutdown();
            log::info!("Native outputs shut down");
        }
    }
}

#[derive(Default)]
pub struct PreviewStore(Mutex<HashMap<&'static str, (u64, Snapshot)>>);

impl PreviewStore {
    pub fn jpeg(&self, label: &str) -> Option<Vec<u8>> {
        self.0.lock().ok()?.get(label).map(|(_, snapshot)| snapshot.jpeg.clone())
    }

    fn store(&self, target: PreviewTarget, snapshot: Snapshot) -> u64 {
        let mut previews = self.0.lock().expect("previews poisoned");
        let revision = previews.get(target.label()).map_or(1, |(revision, _)| revision + 1);
        previews.insert(target.label(), (revision, snapshot));
        revision
    }
}

#[cfg(target_os = "linux")]
fn keep_gtk_from_painting<R: Runtime>(window: &Window<R>, role: OutputRole, handle: tauri::AppHandle<R>) -> anyhow::Result<()> {
    use gtk::prelude::WidgetExt;
    let gtk_window = window.gtk_window()?;
    gtk_window.set_app_paintable(true);
    gtk_window.connect_draw(move |_, _| {
        if let Some(outputs) = handle.try_state::<NativeOutputs>() {
            outputs.redraw(role);
        }
        gtk::glib::Propagation::Stop
    });
    gtk_window.queue_draw();
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn refullscreen_on_map<R: Runtime>(window: &Window<R>, wants_fullscreen: impl Fn() -> bool + 'static) -> anyhow::Result<()> {
    use gtk::prelude::{GtkWindowExt, WidgetExtManual};
    window.gtk_window()?.connect_map_event(move |window, _| {
        if wants_fullscreen() {
            window.fullscreen();
        }
        gtk::glib::Propagation::Proceed
    });
    Ok(())
}

pub fn role_for_label(label: &str) -> Option<OutputRole> {
    match label {
        "projection" => Some(OutputRole::Projection),
        "confidence" => Some(OutputRole::Confidence),
        _ => None,
    }
}

pub fn build_output_window<R: Runtime, M: Manager<R>>(manager: &M, label: &str, title: &str, (width, height): (u32, u32)) -> tauri::Result<Window<R>> {
    tauri::window::WindowBuilder::new(manager, label)
        .title(title)
        .visible(false)
        .decorations(false)
        .shadow(false)
        .skip_taskbar(true)
        .inner_size(width as f64, height as f64)
        .build()
}

pub fn start<R: Runtime>(app: &tauri::App<R>, windows: [&Window<R>; 2], frames: Arc<dyn VideoFrames>) -> anyhow::Result<()> {
    let frames: Arc<dyn VideoFrames> = if std::env::var("MAGI_TEST_PATTERN").as_deref() == Ok("1") {
        Arc::new(TestPatternSource::new(960, 540))
    } else {
        frames
    };
    let stall = std::env::var("MAGI_CONFIDENCE_STALL_MS")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map(Duration::from_millis);

    let mut specs = Vec::new();
    for window in windows {
        #[cfg(target_os = "linux")]
        gtk::prelude::WidgetExt::realize(&window.gtk_window()?);
        let role = role_for_label(window.label()).expect("native output label");
        let size = window.inner_size()?;
        specs.push(OutputSpec {
            role,
            target: Box::new(window.clone()),
            width: size.width,
            height: size.height,
            stall: if role == OutputRole::Confidence { stall } else { None },
        });
    }
    let engine = RenderEngine::start(specs, frames)?;
    app.manage(PreviewStore::default());
    app.manage(monitor_window::Monitors::default());
    let handle = app.handle().clone();
    engine.start_previews(Box::new(move |target, snapshot| {
        let (width, height) = (snapshot.width, snapshot.height);
        log::debug!("{} preview: {width}x{height}, {} bytes", target.label(), snapshot.jpeg.len());
        let revision = handle.state::<PreviewStore>().store(target, snapshot);
        let event = serde_json::json!({ "target": target.label(), "revision": revision, "width": width, "height": height });
        let _ = handle.emit_to("main", "output:preview", event);
    }))?;
    let handle = app.handle().clone();
    engine.on_status(Box::new(move |role, loading| {
        let _ = handle.emit_to("main", "output:status", serde_json::json!({ "target": role.label(), "loading": loading }));
    }));
    app.manage(NativeOutputs(Mutex::new(Some(engine))));

    for window in windows {
        let role = role_for_label(window.label()).expect("native output label");
        #[cfg(target_os = "linux")]
        keep_gtk_from_painting(window, role, app.handle().clone())?;
        let handle = app.handle().clone();
        window.on_window_event(move |event| match event {
            tauri::WindowEvent::Resized(size) => {
                handle.state::<NativeOutputs>().resize(role, size.width, size.height);
            }
            tauri::WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                handle.state::<NativeOutputs>().resize(role, new_inner_size.width, new_inner_size.height);
            }
            tauri::WindowEvent::CloseRequested { api, .. } => api.prevent_close(),
            _ => {}
        });
    }
    Ok(())
}
