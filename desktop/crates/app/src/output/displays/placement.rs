use tauri::{Emitter, Manager};

use crate::output::display_names::{display_names, readable_name};
use crate::output::NativeOutputs;
use crate::system::awake;

use super::*;

pub fn content_size(window: &tauri::Window, size: tauri::PhysicalSize<u32>) -> serde_json::Value {
    let aspect = window.app_handle().try_state::<OutputRoutes>().and_then(|routes| routes.values.lock().ok()?.get(window.label())?.aspect);
    let [_, _, width, height] = render_engine::fit_aspect((size.width, size.height), aspect);
    serde_json::json!({ "label": window.label(), "width": width, "height": height })
}
pub(super) fn present_output(app: &tauri::AppHandle, output: &str) -> Result<(), String> {
    let placed = place_output(app, output);
    if let Some(awake) = app.try_state::<awake::KeepAwake>() {
        let shown = OUTPUTS.iter().any(|(label, _)| app.get_window(label).is_some_and(|w| w.is_visible().unwrap_or(false)));
        awake.set(shown);
    }
    placed
}
pub(super) fn place_output(app: &tauri::AppHandle, output: &str) -> Result<(), String> {
    let window = app.get_window(output).ok_or("Output window unavailable")?;
    let role = crate::output::role_for_label(output).ok_or("Unknown output")?;
    let routes = app.state::<OutputRoutes>();
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let names = display_names();
    let (route, index) = resolve_route(app, &*routes.values.lock().map_err(|e| e.to_string())?, output, &monitors, &names);
    let outputs = app.state::<NativeOutputs>();
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
    window.set_fullscreen(route.fullscreen).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_skip_taskbar(true).map_err(|e| e.to_string())?;
    outputs.set_shown(role, true);
    log_placement(output, target, &names, route.fullscreen);
    if let Ok(size) = window.inner_size() {
        let _ = app.emit_to("main", "output:resized", content_size(&window, size));
    }
    Ok(())
}
pub(super) fn log_placement(output: &str, target: &tauri::Monitor, names: &Names, fullscreen: bool) {
    let system = target.name().map(String::as_str);
    let name = readable_name(system, system.and_then(|n| names.get(n)));
    let size = target.size();
    let hz = system.and_then(crate::output::display_names::refresh_rate);
    let rate = hz.map(|hz| format!(", {hz} Hz")).unwrap_or_default();
    let mode = if fullscreen { "fullscreen" } else { "windowed" };
    log::info!("{output} output on {name} ({}×{}{rate}, {mode})", size.width, size.height);
    if let Some(hz) = hz.filter(|hz| *hz < 59) {
        log::warn!("{name} refreshes at {hz} Hz, so the {output} output cannot pass {hz} fps; set it to 60 Hz in Windows display settings");
    }
}
