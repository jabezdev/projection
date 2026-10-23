//! Output windows on the operator's displays: saved routes (which display,
//! fullscreen or windowed, size and aspect), showing and hiding outputs, and
//! the preview stills the operator UI shows.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::native_output::{self, NativeOutputs};
use crate::{native_scene, settings};

/// Where and how an output window appears. Saved routes from before the
/// enable, size and aspect choices load with the defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputRoute {
    name: Option<String>, x: i32, y: i32, width: u32, height: u32,
    #[serde(default = "yes")] fullscreen: bool,
    #[serde(default = "yes")] enabled: bool,
    /// Window size when not fullscreen.
    #[serde(default)] size: Option<(u32, u32)>,
    /// Content aspect ratio, letterboxed; `None` fills the screen.
    #[serde(default)] aspect: Option<(u32, u32)>,
}
fn yes() -> bool { true }
pub const OUTPUTS: [(&str, &str); 2] = [("projection", "PAKIT Projection"), ("confidence", "PAKIT Confidence")];
pub const WINDOWED_SIZE: (u32, u32) = (1280, 720);
pub struct OutputRoutes {
    pub values: Mutex<std::collections::BTreeMap<String, OutputRoute>>,
    pub path: std::path::PathBuf,
    /// The global switch; outputs always start hidden.
    pub shown: AtomicBool,
}
impl OutputRoutes {
    /// Whether `output` should be fullscreen when its window maps.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn wants_fullscreen(&self, output: &str) -> bool {
        self.values.lock().ok().is_some_and(|saved| saved.get(output).is_none_or(|route| route.fullscreen))
    }
}
fn matching_route(route: &OutputRoute, candidates: &[OutputRoute]) -> Option<usize> {
    candidates.iter().position(|m| m.name == route.name && m.x == route.x && m.y == route.y)
        .or_else(|| {
            let matches: Vec<_> = candidates.iter().enumerate().filter(|(_, m)| route.name.is_some() && m.name == route.name).map(|(i, _)| i).collect();
            if matches.len() == 1 { Some(matches[0]) } else {
                let matches: Vec<_> = candidates.iter().enumerate().filter(|(_, m)| m.name == route.name && m.width == route.width && m.height == route.height).map(|(i, _)| i).collect();
                if matches.len() == 1 { Some(matches[0]) } else { None }
            }
        })
}
fn monitor_route(monitor: &tauri::Monitor) -> OutputRoute {
    OutputRoute { name: monitor.name().cloned(), x: monitor.position().x, y: monitor.position().y, width: monitor.size().width, height: monitor.size().height, fullscreen: true, enabled: true, size: None, aspect: None }
}
fn matching_monitor(route: &OutputRoute, monitors: &[tauri::Monitor]) -> Option<usize> {
    let candidates: Vec<_> = monitors.iter().map(monitor_route).collect();
    matching_route(route, &candidates)
}
/// Unconfigured outputs take the displays the operator window is not on, in
/// order, and share the operator's display when there are too few.
fn default_monitor(output: &str, operator: Option<usize>, count: usize) -> usize {
    let others: Vec<_> = (0..count).filter(|&i| Some(i) != operator).collect();
    let rank = if output == "projection" { 0 } else { 1 };
    others.get(rank).copied().or(operator).unwrap_or(0)
}
/// The saved route, or the default one, and its display's index (`None`
/// when the saved display is disconnected).
fn resolve_route(app: &tauri::AppHandle, saved: &std::collections::BTreeMap<String, OutputRoute>, output: &str, monitors: &[tauri::Monitor]) -> (OutputRoute, Option<usize>) {
    if let Some(route) = saved.get(output) {
        return (route.clone(), matching_monitor(route, monitors));
    }
    let operator = app.get_webview_window("main").and_then(|w| w.current_monitor().ok().flatten()).and_then(|m| matching_monitor(&monitor_route(&m), monitors));
    let index = default_monitor(output, operator, monitors.len());
    match monitors.get(index) {
        Some(monitor) => (monitor_route(monitor), Some(index)),
        None => (OutputRoute { name: None, x: 0, y: 0, width: 0, height: 0, fullscreen: true, enabled: true, size: None, aspect: None }, None),
    }
}
/// The size previews draw at: the window's, letterboxed to the aspect.
pub fn content_size(window: &tauri::Window, size: tauri::PhysicalSize<u32>) -> serde_json::Value {
    let aspect = window.app_handle().try_state::<OutputRoutes>().and_then(|routes| routes.values.lock().ok()?.get(window.label())?.aspect);
    let [_, _, width, height] = render_engine::fit_aspect((size.width, size.height), aspect);
    serde_json::json!({ "label": window.label(), "width": width, "height": height })
}
/// Shows `output` on its display as a borderless window, or hides it, and
/// keeps the displays awake while any output is shown.
fn present_output(app: &tauri::AppHandle, output: &str) -> Result<(), String> {
    let placed = place_output(app, output);
    if let Some(awake) = app.try_state::<crate::awake::KeepAwake>() {
        let shown = OUTPUTS.iter().any(|(label, _)| app.get_window(label).is_some_and(|w| w.is_visible().unwrap_or(false)));
        awake.set(shown);
    }
    placed
}
fn place_output(app: &tauri::AppHandle, output: &str) -> Result<(), String> {
    let window = app.get_window(output).ok_or("Output window unavailable")?;
    let role = native_output::role_for_label(output).ok_or("Unknown output")?;
    let routes = app.state::<OutputRoutes>();
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let (route, index) = resolve_route(app, &*routes.values.lock().map_err(|e| e.to_string())?, output, &monitors);
    let outputs = app.state::<NativeOutputs>();
    // A disconnected saved display keeps its output hidden rather than
    // unexpectedly covering another screen.
    let target = index.and_then(|i| monitors.get(i)).filter(|_| route.enabled && routes.shown.load(Ordering::Acquire));
    let Some(target) = target else {
        outputs.set_shown(role, false);
        return window.hide().map_err(|e| e.to_string());
    };
    outputs.set_aspect(role, route.aspect);
    window.set_decorations(false).map_err(|e| e.to_string())?;
    if !route.fullscreen {
        let (width, height) = route.size.unwrap_or(WINDOWED_SIZE);
        window.set_size(tauri::PhysicalSize::new(width, height)).map_err(|e| e.to_string())?;
    }
    window.set_position(*target.position()).map_err(|e| e.to_string())?;
    // Requested before mapping, fullscreen is the window's initial state;
    // requested after, some window managers miss it.
    window.set_fullscreen(route.fullscreen).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    // Windows can restore the taskbar button of a window shown after being
    // hidden; outputs never have one, so the app is one taskbar entry.
    window.set_skip_taskbar(true).map_err(|e| e.to_string())?;
    outputs.set_shown(role, true);
    if let Ok(size) = window.inner_size() {
        let _ = app.emit_to("main", "output:resized", content_size(&window, size));
    }
    Ok(())
}
#[derive(Serialize)]
pub struct OutputDisplay { monitor: Option<usize>, enabled: bool, fullscreen: bool, size: Option<(u32, u32)>, aspect: Option<(u32, u32)> }
#[derive(Serialize)]
pub struct OutputDisplays { monitors: Vec<String>, shown: bool, outputs: std::collections::BTreeMap<String, OutputDisplay> }
fn monitor_label(index: usize, monitor: &tauri::Monitor) -> String {
    format!("{}: {} ({}×{})", index + 1, monitor.name().map(String::as_str).unwrap_or("Display"), monitor.size().width, monitor.size().height)
}
#[tauri::command]
pub fn get_output_displays(app: tauri::AppHandle, routes: tauri::State<OutputRoutes>) -> Result<OutputDisplays, String> {
    let window = app.get_webview_window("main").ok_or("Control window unavailable")?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let saved = routes.values.lock().map_err(|e| e.to_string())?;
    let outputs = OUTPUTS.iter().map(|(output, _)| {
        let (route, monitor) = resolve_route(&app, &saved, output, &monitors);
        (output.to_string(), OutputDisplay { monitor, enabled: route.enabled, fullscreen: route.fullscreen, size: route.size, aspect: route.aspect })
    }).collect();
    Ok(OutputDisplays { monitors: monitors.iter().enumerate().map(|(i, m)| monitor_label(i, m)).collect(), shown: routes.shown.load(Ordering::Acquire), outputs })
}
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn set_output_display(app: tauri::AppHandle, routes: tauri::State<OutputRoutes>, output: String, monitor: usize, label: String, enabled: bool, fullscreen: bool, size: Option<(u32, u32)>, aspect: Option<(u32, u32)>) -> Result<OutputDisplays, String> {
    if !OUTPUTS.iter().any(|(name, _)| *name == output) { return Err("Unknown output".into()); }
    if size.is_some_and(|(w, h)| w == 0 || h == 0) || aspect.is_some_and(|(w, h)| w == 0 || h == 0) { return Err("Sizes and aspect ratios must not be zero".into()); }
    let window = app.get_webview_window("main").ok_or("Control window unavailable")?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let target = monitors.get(monitor).ok_or("Display is no longer connected")?;
    // The list may have changed since the operator chose from it.
    if label != monitor_label(monitor, target) {
        return Err("The connected displays changed. Choose the display again.".into());
    }
    {
        let mut saved = routes.values.lock().map_err(|e| e.to_string())?;
        saved.insert(output.clone(), OutputRoute { fullscreen, enabled, size, aspect, ..monitor_route(target) });
        settings::write_atomic(&routes.path, &*saved).map_err(|e| e.to_string())?;
    }
    present_output(&app, &output)?;
    get_output_displays(app, routes)
}
/// Test aid (`PAKIT_SHOW_OUTPUTS=1`): shows the outputs at startup, as the
/// operator's Show button would, so soak runs need no click.
pub fn show_outputs_if_requested(app: &tauri::AppHandle) {
    if std::env::var("PAKIT_SHOW_OUTPUTS").as_deref() != Ok("1") { return; }
    app.state::<OutputRoutes>().shown.store(true, Ordering::Release);
    let _ = app.emit("output:shown", true);
    for (output, _) in OUTPUTS {
        if let Err(e) = present_output(app, output) { log::warn!("Showing {output}: {e}"); }
    }
}
#[tauri::command]
pub fn set_outputs_shown(app: tauri::AppHandle, routes: tauri::State<OutputRoutes>, shown: bool) -> Result<OutputDisplays, String> {
    routes.shown.store(shown, Ordering::Release);
    let _ = app.emit("output:shown", shown);
    for (output, _) in OUTPUTS {
        present_output(&app, output)?;
    }
    get_output_displays(app, routes)
}

/// The latest native preview still of an output ("projection", "confidence")
/// or of the style sample ("style"), as JPEG bytes.
#[tauri::command]
pub fn get_output_preview(previews: tauri::State<native_output::PreviewStore>, target: String) -> Result<tauri::ipc::Response, String> {
    previews.jpeg(&target).map(tauri::ipc::Response::new).ok_or_else(|| format!("No {target} preview yet"))
}

/// Renders the settings dialog's samples of both screens in `config` natively.
#[tauri::command]
pub fn preview_output_style(outputs: tauri::State<NativeOutputs>, config: protocol::OutputConfig) -> Result<(), String> {
    config.validate()?;
    outputs.preview_style(render_engine::Scene::Main(native_scene::style_sample_scene(&config)));
    outputs.preview_style(render_engine::Scene::Confidence(native_scene::confidence_sample_scene(&config)));
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

#[cfg(test)]
mod tests {
    use super::*;
    fn display(name: &str, x: i32) -> OutputRoute { OutputRoute { name: Some(name.into()), x, y: 0, width: 1920, height: 1080, fullscreen: true, enabled: true, size: None, aspect: None } }
    #[test]
    fn restores_named_display_after_enumeration_or_position_changes() {
        let audience = display("HDMI-A-1", 1920);
        assert_eq!(matching_route(&audience, &[audience.clone(), display("eDP-1", 0)]), Some(0));
        assert_eq!(matching_route(&audience, &[display("eDP-1", 0), display("HDMI-A-1", 0)]), Some(1));
    }
    #[test]
    fn missing_or_ambiguous_display_does_not_redirect_output() {
        let audience = display("HDMI-A-1", 1920);
        assert_eq!(matching_route(&audience, &[display("eDP-1", 0)]), None);
        assert_eq!(matching_route(&audience, &[display("HDMI-A-1", 0), display("HDMI-A-1", 3840)]), None);
    }
    #[test]
    fn saved_routes_preserve_separate_fullscreen_choices() {
        let audience = display("HDMI-A-1", 1920);
        let mut confidence = display("DP-1", -1920);
        confidence.fullscreen = false;
        let routes = std::collections::BTreeMap::from([("projection", audience), ("confidence", confidence)]);
        let loaded: std::collections::BTreeMap<String, OutputRoute> = serde_json::from_slice(&serde_json::to_vec(&routes).unwrap()).unwrap();
        assert!(loaded["projection"].fullscreen);
        assert!(!loaded["confidence"].fullscreen);
        assert_eq!(loaded["confidence"].name.as_deref(), Some("DP-1"));
    }
    #[test]
    fn routes_saved_before_display_options_enable_both_outputs() {
        let route: OutputRoute = serde_json::from_str(r#"{"name":"HDMI-A-1","x":0,"y":0,"width":1920,"height":1080,"fullscreen":false}"#).unwrap();
        assert!(route.enabled && !route.fullscreen);
        assert_eq!((route.size, route.aspect), (None, None));
    }
    #[test]
    fn default_displays_avoid_the_operator_screen() {
        assert_eq!((default_monitor("projection", Some(0), 3), default_monitor("confidence", Some(0), 3)), (1, 2));
        assert_eq!((default_monitor("projection", Some(1), 2), default_monitor("confidence", Some(1), 2)), (0, 1));
        assert_eq!((default_monitor("projection", Some(0), 1), default_monitor("confidence", None, 1)), (0, 0));
    }
}
