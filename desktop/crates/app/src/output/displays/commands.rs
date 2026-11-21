
use serde::Serialize;
use tauri::{Emitter, Manager};


use crate::output::display_names::{aspect_label, display_names, readable_name, system_number};
use crate::output::NativeOutputs;
use crate::settings;
use crate::output::{monitor_window, scene};

use super::*;

#[derive(Serialize)]
pub struct OutputDisplay { monitor: Option<usize>, shown: bool, fullscreen: bool, size: Option<(u32, u32)>, aspect: Option<(u32, u32)> }
/// A connected display as the operator sees it.
#[derive(Serialize)]
pub struct MonitorInfo {
    /// Identifies this entry of this list; a choice made from an older list
    /// is refused.
    pub(super) key: String,
    /// The monitor's model name ("BenQ GW2480"), else "Display 2".
    pub(super) name: String,
    /// "HDMI", "DisplayPort", "Built-in"…, when the system reports it.
    pub(super) connector: Option<&'static str>,
    pub(super) width: u32,
    pub(super) height: u32,
    /// "16:9", "16:10"…
    pub(super) aspect: String,
    /// The operator window is on this display.
    pub(super) operator: bool,
}
#[derive(Serialize)]
pub struct OutputDisplays { monitors: Vec<MonitorInfo>, /// Any output is shown.
    shown: bool, outputs: std::collections::BTreeMap<String, OutputDisplay> }
pub(super) fn monitor_key(index: usize, monitor: &tauri::Monitor) -> String {
    format!("{index}|{}|{}x{}", monitor.name().map(String::as_str).unwrap_or(""), monitor.size().width, monitor.size().height)
}
pub(super) fn monitor_infos(monitors: &[tauri::Monitor], names: &Names, operator: Option<usize>) -> Vec<MonitorInfo> {
    let readable: Vec<String> = monitors.iter().map(|m| readable_name(m.name().map(String::as_str), m.name().and_then(|n| names.get(n)))).collect();
    monitors.iter().enumerate().map(|(index, monitor)| {
        let mut name = readable[index].clone();
        // Two identical projectors: tell them apart by Windows' number.
        if readable.iter().filter(|other| **other == name).count() > 1 {
            let number = monitor.name().and_then(|n| system_number(n)).unwrap_or(index as u32 + 1);
            name = format!("{name} (Display {number})");
        }
        let info = monitor.name().and_then(|n| names.get(n));
        let (width, height) = (monitor.size().width, monitor.size().height);
        MonitorInfo { key: monitor_key(index, monitor), name, connector: info.and_then(|i| i.connector), width, height, aspect: aspect_label(width, height), operator: operator == Some(index) }
    }).collect()
}
#[tauri::command]
pub fn get_output_displays(app: tauri::AppHandle, routes: tauri::State<OutputRoutes>) -> Result<OutputDisplays, String> {
    let window = app.get_webview_window("main").ok_or("Control window unavailable")?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let names = display_names();
    let saved = routes.values.lock().map_err(|e| e.to_string())?;
    let outputs = OUTPUTS.iter().map(|(output, _)| {
        let (route, monitor) = resolve_route(&app, &saved, output, &monitors, &names);
        (output.to_string(), OutputDisplay { monitor, shown: monitor.is_some() && routes.is_shown(output), fullscreen: route.fullscreen, size: route.size, aspect: route.aspect })
    }).collect();
    let operator = operator_monitor(&app, &monitors, &names);
    Ok(OutputDisplays { monitors: monitor_infos(&monitors, &names, operator), shown: routes.any_shown(), outputs })
}
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn set_output_display(app: tauri::AppHandle, routes: tauri::State<OutputRoutes>, output: String, monitor: usize, key: String, fullscreen: bool, size: Option<(u32, u32)>, aspect: Option<(u32, u32)>) -> Result<OutputDisplays, String> {
    if !OUTPUTS.iter().any(|(name, _)| *name == output) { return Err("Unknown output".into()); }
    if size.is_some_and(|(w, h)| w == 0 || h == 0) || aspect.is_some_and(|(w, h)| w == 0 || h == 0) { return Err("Sizes and aspect ratios must not be zero".into()); }
    let window = app.get_webview_window("main").ok_or("Control window unavailable")?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let target = monitors.get(monitor).ok_or("Display is no longer connected")?;
    // The list may have changed since the operator chose from it.
    if key != monitor_key(monitor, target) {
        return Err("The connected displays changed. Choose the display again.".into());
    }
    {
        let names = display_names();
        let mut saved = routes.values.lock().map_err(|e| e.to_string())?;
        saved.insert(output.clone(), OutputRoute { fullscreen, size, aspect, ..monitor_route(target, &names) });
        settings::write_atomic(&routes.path, &*saved).map_err(|e| e.to_string())?;
    }
    present_output(&app, &output)?;
    get_output_displays(app, routes)
}
/// Test aid (`PALTO_SHOW_OUTPUTS=1`): shows the outputs at startup, as the
/// operator's screen switches would, so soak runs need no click.
pub fn show_outputs_if_requested(app: &tauri::AppHandle) {
    if std::env::var("PALTO_SHOW_OUTPUTS").as_deref() != Ok("1") { return; }
    let routes = app.state::<OutputRoutes>();
    for (output, _) in OUTPUTS {
        routes.set_shown(output, true);
        if let Err(e) = present_output(app, output) { log::warn!("Showing {output}: {e}"); }
    }
    let _ = app.emit("output:shown", routes.any_shown());
}
/// Shows or hides one output on its display.
#[tauri::command]
pub fn set_output_shown(app: tauri::AppHandle, routes: tauri::State<OutputRoutes>, output: String, shown: bool) -> Result<OutputDisplays, String> {
    if !OUTPUTS.iter().any(|(name, _)| *name == output) { return Err("Unknown output".into()); }
    routes.set_shown(&output, shown);
    let presented = present_output(&app, &output);
    let _ = app.emit("output:shown", routes.any_shown());
    presented?;
    get_output_displays(app, routes)
}

/// The latest native preview still of an output ("projection", "confidence")
/// or of the style sample ("style"), as JPEG bytes.
#[tauri::command]
pub fn get_output_preview(previews: tauri::State<crate::output::PreviewStore>, target: String) -> Result<tauri::ipc::Response, String> {
    previews.jpeg(&target).map(tauri::ipc::Response::new).ok_or_else(|| format!("No {target} preview yet"))
}

/// Shows the live monitor of `target` ("projection", "confidence") at `rect`
/// (physical pixels in the operator webview), or hides it with `None`.
/// Resolves false when live monitors are unavailable; the UI then keeps its
/// still previews.
#[tauri::command]
pub async fn set_output_monitor(app: tauri::AppHandle, target: String, rect: Option<monitor_window::Rect>) -> Result<bool, String> {
    let role = crate::output::role_for_label(&target).ok_or_else(|| format!("Unknown output {target}"))?;
    let (sent, received) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    // Child windows belong to the operator window's thread.
    app.run_on_main_thread(move || {
        let shown = handle.try_state::<monitor_window::Monitors>().is_some_and(|monitors| monitors.place(&handle, role, rect));
        let _ = sent.send(shown);
    })
    .map_err(|e| e.to_string())?;
    received.await.map_err(|e| e.to_string())
}

/// Renders the settings dialog's samples of both screens in `config` natively.
#[tauri::command]
pub fn preview_output_style(outputs: tauri::State<NativeOutputs>, config: protocol::OutputConfig) -> Result<(), String> {
    config.validate()?;
    outputs.preview_style(render_engine::Scene::Main(scene::style_sample_scene(&config)));
    outputs.preview_style(render_engine::Scene::Confidence(scene::confidence_sample_scene(&config)));
    Ok(())
}

/// Current output dimensions for matching operator preview aspect ratios.
#[tauri::command]
pub fn get_output_sizes(app: tauri::AppHandle) -> serde_json::Value {
    let mut sizes = serde_json::Map::new();
    for (label, _) in OUTPUTS {
        if let Some(window) = app.get_window(label) {
            if let Ok(size) = window.inner_size() { sizes.insert(label.into(), content_size(&window, size)); }
        }
    }
    serde_json::Value::Object(sizes)
}
