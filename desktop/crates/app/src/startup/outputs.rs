use std::path::Path;
use std::sync::{Arc, RwLock};

use tauri::{Emitter, Manager, Window};
use tokio::sync::broadcast::error::RecvError;

use super::Channels;
use crate::capture::Captures;
use crate::displays::{content_size, show_outputs_if_requested, OutputRoutes, OUTPUTS, WINDOWED_SIZE};
use crate::native_media::NativeMedia;
use crate::native_output::{self, NativeOutputs};
use crate::native_scene::SceneFeed;
use crate::settings::{AppConfig, OutputState};

/// Opens the projection and confidence windows (hidden until the operator
/// shows them), starts the shared media session and the scene feed that keeps
/// both outputs in step with the engine.
pub(super) fn start(app: &tauri::App, channels: &Channels, config: &Arc<RwLock<AppConfig>>, dir: &Path) -> anyhow::Result<Arc<NativeMedia>> {
    let projection_size = config.read().expect("config lock poisoned").projection_size.unwrap_or(WINDOWED_SIZE);
    let projection = native_output::build_output_window(app, OUTPUTS[0].0, OUTPUTS[0].1, projection_size)?;
    let confidence = native_output::build_output_window(app, OUTPUTS[1].0, OUTPUTS[1].1, WINDOWED_SIZE)?;
    let windows = [&projection, &confidence];
    #[cfg(target_os = "linux")]
    for window in windows {
        let (handle, label) = (app.handle().clone(), window.label().to_string());
        native_output::refullscreen_on_map(window, move || handle.state::<OutputRoutes>().wants_fullscreen(&label))?;
    }
    for window in windows {
        report_resizes_to_operator(app.handle().clone(), window);
    }

    let media = NativeMedia::start(channels.engine.clone())?;
    app.manage(media.clone());
    let captures = Arc::new(Captures::new(&config.read().expect("config lock poisoned").capture_method, dir));
    app.manage(captures.clone());
    native_output::start(app, windows, media.frames(captures.clone()))?;

    let feed = start_scene_feed(app, channels, &media, captures)?;
    app.manage(feed);
    show_outputs_if_requested(app.handle());
    remember_projection_size(&projection, config.clone());
    Ok(media)
}

fn report_resizes_to_operator(app: tauri::AppHandle, window: &Window) {
    let output = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Resized(size) = event {
            let _ = app.emit_to("main", "output:resized", content_size(&output, *size));
        }
    });
}

fn start_scene_feed(app: &tauri::App, channels: &Channels, media: &Arc<NativeMedia>, captures: Arc<Captures>) -> anyhow::Result<SceneFeed> {
    let style_source = app.handle().clone();
    let outputs = app.handle().clone();
    let media = media.clone();
    let feed = SceneFeed::start(
        channels.snapshot.clone(),
        move || style_source.state::<OutputState>().config.lock().map(|c| c.clone()).unwrap_or_default(),
        move |scenes, plan| {
            captures.set_active(&plan.captures);
            media.set_ambient(&plan.ambient, &plan.prepared_ambient);
            media.prepare(plan.foreground.as_deref());
            if let Some(outputs) = outputs.try_state::<NativeOutputs>() {
                for (role, scene) in scenes {
                    outputs.set_scene(role, scene);
                }
                outputs.preload(&plan.images);
            }
        },
    )?;

    let refreshed = feed.clone();
    let mut events = channels.events.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(_) | Err(RecvError::Lagged(_)) => refreshed.refresh(),
                Err(RecvError::Closed) => break,
            }
        }
    });
    Ok(feed)
}

fn remember_projection_size(projection: &Window, config: Arc<RwLock<AppConfig>>) {
    projection.on_window_event(move |event| {
        if let tauri::WindowEvent::Resized(size) = event {
            if let Ok(mut config) = config.write() {
                config.projection_size = Some((size.width, size.height));
            }
        }
    });
}
