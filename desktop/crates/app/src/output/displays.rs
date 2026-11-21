//! Output windows on the operator's displays: saved routes (which display,
//! fullscreen or windowed, size and aspect), showing and hiding outputs, and
//! the preview stills the operator UI shows.

use std::collections::BTreeSet;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use std::collections::HashMap;

use crate::output::display_names::{aspect_label, display_names, readable_name, system_number, DisplayName};
use crate::output::NativeOutputs;
use crate::settings;
use super::{display_names, monitor_window, scene};
use crate::system::awake;

/// Where and how an output window appears. Saved routes from before the
/// size and aspect choices load with the defaults; an `enabled` field from
/// older files is ignored (showing is chosen per screen at runtime).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputRoute {
    name: Option<String>, x: i32, y: i32, width: u32, height: u32,
    #[serde(default = "yes")] fullscreen: bool,
    /// Window size when not fullscreen.
    #[serde(default)] size: Option<(u32, u32)>,
    /// Content aspect ratio, letterboxed; `None` fills the screen.
    #[serde(default)] aspect: Option<(u32, u32)>,
    /// Windows: the monitor's device path, which survives the renumbering of
    /// `\\.\DISPLAYn` names when displays are replugged.
    #[serde(default, skip_serializing_if = "Option::is_none")] device: Option<String>,
}
fn yes() -> bool { true }
pub const OUTPUTS: [(&str, &str); 2] = [("projection", "PALTO Projection"), ("confidence", "PALTO Confidence")];
pub const WINDOWED_SIZE: (u32, u32) = (1280, 720);
pub struct OutputRoutes {
    pub values: Mutex<std::collections::BTreeMap<String, OutputRoute>>,
    pub path: std::path::PathBuf,
    /// Outputs the operator has shown. Not saved: outputs always start hidden.
    pub shown: Mutex<BTreeSet<String>>,
}
impl OutputRoutes {
    pub fn new(values: std::collections::BTreeMap<String, OutputRoute>, path: std::path::PathBuf) -> Self {
        Self { values: Mutex::new(values), path, shown: Mutex::default() }
    }
    pub fn is_shown(&self, output: &str) -> bool {
        self.shown.lock().is_ok_and(|shown| shown.contains(output))
    }
    /// Whether any output is shown (updates wait; the UI marks it).
    pub fn any_shown(&self) -> bool {
        self.shown.lock().is_ok_and(|shown| !shown.is_empty())
    }
    fn set_shown(&self, output: &str, shown: bool) {
        if let Ok(mut set) = self.shown.lock() {
            if shown { set.insert(output.to_string()); } else { set.remove(output); }
        }
    }
    /// Whether `output` should be fullscreen when its window maps.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn wants_fullscreen(&self, output: &str) -> bool {
        self.values.lock().ok().is_some_and(|saved| saved.get(output).is_none_or(|route| route.fullscreen))
    }
}
fn matching_route(route: &OutputRoute, candidates: &[OutputRoute]) -> Option<usize> {
    if route.device.is_some() {
        let matches: Vec<_> = candidates.iter().enumerate().filter(|(_, m)| m.device == route.device).map(|(i, _)| i).collect();
        if matches.len() == 1 { return Some(matches[0]); }
        // Windows can reuse a disconnected display's old number. Once
        // device identities are available, a name match is not enough.
        if candidates.iter().any(|m| m.device.is_some()) { return None; }
    }
    candidates.iter().position(|m| m.name == route.name && m.x == route.x && m.y == route.y)
        .or_else(|| {
            let matches: Vec<_> = candidates.iter().enumerate().filter(|(_, m)| route.name.is_some() && m.name == route.name).map(|(i, _)| i).collect();
            if matches.len() == 1 { Some(matches[0]) } else {
                let matches: Vec<_> = candidates.iter().enumerate().filter(|(_, m)| m.name == route.name && m.width == route.width && m.height == route.height).map(|(i, _)| i).collect();
                if matches.len() == 1 { Some(matches[0]) } else { None }
            }
        })
}
type Names = HashMap<String, DisplayName>;
fn monitor_route(monitor: &tauri::Monitor, names: &Names) -> OutputRoute {
    let device = monitor.name().and_then(|name| names.get(name)).and_then(|info| info.device.clone());
    OutputRoute { name: monitor.name().cloned(), x: monitor.position().x, y: monitor.position().y, width: monitor.size().width, height: monitor.size().height, fullscreen: true, size: None, aspect: None, device }
}
fn matching_monitor(route: &OutputRoute, monitors: &[tauri::Monitor], names: &Names) -> Option<usize> {
    let candidates: Vec<_> = monitors.iter().map(|m| monitor_route(m, names)).collect();
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
fn resolve_route(app: &tauri::AppHandle, saved: &std::collections::BTreeMap<String, OutputRoute>, output: &str, monitors: &[tauri::Monitor], names: &Names) -> (OutputRoute, Option<usize>) {
    if let Some(route) = saved.get(output) {
        return (route.clone(), matching_monitor(route, monitors, names));
    }
    let index = default_monitor(output, operator_monitor(app, monitors, names), monitors.len());
    match monitors.get(index) {
        Some(monitor) => (monitor_route(monitor, names), Some(index)),
        None => (OutputRoute { name: None, x: 0, y: 0, width: 0, height: 0, fullscreen: true, size: None, aspect: None, device: None }, None),
    }
}
/// The display the operator window is on.
fn operator_monitor(app: &tauri::AppHandle, monitors: &[tauri::Monitor], names: &Names) -> Option<usize> {
    app.get_webview_window("main").and_then(|w| w.current_monitor().ok().flatten()).and_then(|m| matching_monitor(&monitor_route(&m, names), monitors, names))
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
    if let Some(awake) = app.try_state::<awake::KeepAwake>() {
        let shown = OUTPUTS.iter().any(|(label, _)| app.get_window(label).is_some_and(|w| w.is_visible().unwrap_or(false)));
        awake.set(shown);
    }
    placed
}
fn place_output(app: &tauri::AppHandle, output: &str) -> Result<(), String> {
    let window = app.get_window(output).ok_or("Output window unavailable")?;
    let role = crate::output::role_for_label(output).ok_or("Unknown output")?;
    let routes = app.state::<OutputRoutes>();
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let names = display_names();
    let (route, index) = resolve_route(app, &*routes.values.lock().map_err(|e| e.to_string())?, output, &monitors, &names);
    let outputs = app.state::<NativeOutputs>();
    // A disconnected saved display keeps its output hidden rather than
    // unexpectedly covering another screen.
    let target = index.and_then(|i| monitors.get(i)).filter(|_| routes.is_shown(output));
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
    log_placement(output, target, &names, route.fullscreen);
    if let Ok(size) = window.inner_size() {
        let _ = app.emit_to("main", "output:resized", content_size(&window, size));
    }
    Ok(())
}
/// Records where an output went. A display below 60 Hz caps the output's
/// motion at its own rate, which no rendering change can lift.
fn log_placement(output: &str, target: &tauri::Monitor, names: &Names, fullscreen: bool) {
    let system = target.name().map(String::as_str);
    let name = readable_name(system, system.and_then(|n| names.get(n)));
    let size = target.size();
    let hz = system.and_then(display_names::refresh_rate);
    let rate = hz.map(|hz| format!(", {hz} Hz")).unwrap_or_default();
    let mode = if fullscreen { "fullscreen" } else { "windowed" };
    log::info!("{output} output on {name} ({}×{}{rate}, {mode})", size.width, size.height);
    if let Some(hz) = hz.filter(|hz| *hz < 59) {
        log::warn!("{name} refreshes at {hz} Hz, so the {output} output cannot pass {hz} fps; set it to 60 Hz in Windows display settings");
    }
}
#[derive(Serialize)]
pub struct OutputDisplay { monitor: Option<usize>, shown: bool, fullscreen: bool, size: Option<(u32, u32)>, aspect: Option<(u32, u32)> }
/// A connected display as the operator sees it.
#[derive(Serialize)]
pub struct MonitorInfo {
    /// Identifies this entry of this list; a choice made from an older list
    /// is refused.
    key: String,
    /// The monitor's model name ("BenQ GW2480"), else "Display 2".
    name: String,
    /// "HDMI", "DisplayPort", "Built-in"…, when the system reports it.
    connector: Option<&'static str>,
    width: u32,
    height: u32,
    /// "16:9", "16:10"…
    aspect: String,
    /// The operator window is on this display.
    operator: bool,
}
#[derive(Serialize)]
pub struct OutputDisplays { monitors: Vec<MonitorInfo>, /// Any output is shown.
    shown: bool, outputs: std::collections::BTreeMap<String, OutputDisplay> }
fn monitor_key(index: usize, monitor: &tauri::Monitor) -> String {
    format!("{index}|{}|{}x{}", monitor.name().map(String::as_str).unwrap_or(""), monitor.size().width, monitor.size().height)
}
fn monitor_infos(monitors: &[tauri::Monitor], names: &Names, operator: Option<usize>) -> Vec<MonitorInfo> {
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

#[cfg(test)]
mod tests {
    use super::*;
    fn display(name: &str, x: i32) -> OutputRoute { OutputRoute { name: Some(name.into()), x, y: 0, width: 1920, height: 1080, fullscreen: true, size: None, aspect: None, device: None } }
    #[test]
    fn follows_a_windows_display_renumbered_after_replugging() {
        let mut projector = display(r"\\.\DISPLAY2", 1920);
        projector.device = Some("projector-path".into());
        let mut renumbered = display(r"\\.\DISPLAY3", 1920);
        renumbered.device = projector.device.clone();
        // The old number now belongs to another screen at the old position.
        assert_eq!(matching_route(&projector, &[display(r"\\.\DISPLAY2", 1920), renumbered]), Some(1));
    }
    #[test]
    fn a_disconnected_device_does_not_use_another_monitors_recycled_number() {
        let mut projector = display(r"\\.\DISPLAY2", 1920);
        projector.device = Some("projector-path".into());
        let mut replacement = display(r"\\.\DISPLAY2", 1920);
        replacement.device = Some("operator-monitor-path".into());
        assert_eq!(matching_route(&projector, &[replacement]), None);
        // If the OS cannot report device paths, retain the older name fallback.
        assert_eq!(matching_route(&projector, &[display(r"\\.\DISPLAY2", 1920)]), Some(0));
    }
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
    fn routes_saved_by_older_versions_still_load() {
        let route: OutputRoute = serde_json::from_str(r#"{"name":"HDMI-A-1","x":0,"y":0,"width":1920,"height":1080,"fullscreen":false,"enabled":false}"#).unwrap();
        assert!(!route.fullscreen);
        assert_eq!((route.size, route.aspect), (None, None));
    }
    #[test]
    fn default_displays_avoid_the_operator_screen() {
        assert_eq!((default_monitor("projection", Some(0), 3), default_monitor("confidence", Some(0), 3)), (1, 2));
        assert_eq!((default_monitor("projection", Some(1), 2), default_monitor("confidence", Some(1), 2)), (0, 1));
        assert_eq!((default_monitor("projection", Some(0), 1), default_monitor("confidence", None, 1)), (0, 0));
    }
}
