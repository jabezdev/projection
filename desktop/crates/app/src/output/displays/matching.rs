use tauri::Manager;

use std::collections::HashMap;

use crate::output::display_names::DisplayName;

use super::*;

pub(super) fn matching_route(route: &OutputRoute, candidates: &[OutputRoute]) -> Option<usize> {
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
pub(super) type Names = HashMap<String, DisplayName>;
pub(super) fn monitor_route(monitor: &tauri::Monitor, names: &Names) -> OutputRoute {
    let device = monitor.name().and_then(|name| names.get(name)).and_then(|info| info.device.clone());
    OutputRoute { name: monitor.name().cloned(), x: monitor.position().x, y: monitor.position().y, width: monitor.size().width, height: monitor.size().height, fullscreen: true, size: None, aspect: None, device }
}
pub(super) fn matching_monitor(route: &OutputRoute, monitors: &[tauri::Monitor], names: &Names) -> Option<usize> {
    let candidates: Vec<_> = monitors.iter().map(|m| monitor_route(m, names)).collect();
    matching_route(route, &candidates)
}
pub(super) fn default_monitor(output: &str, operator: Option<usize>, count: usize) -> usize {
    let others: Vec<_> = (0..count).filter(|&i| Some(i) != operator).collect();
    let rank = if output == "projection" { 0 } else { 1 };
    others.get(rank).copied().or(operator).unwrap_or(0)
}
pub(super) fn resolve_route(app: &tauri::AppHandle, saved: &std::collections::BTreeMap<String, OutputRoute>, output: &str, monitors: &[tauri::Monitor], names: &Names) -> (OutputRoute, Option<usize>) {
    if let Some(route) = saved.get(output) {
        return (route.clone(), matching_monitor(route, monitors, names));
    }
    let index = default_monitor(output, operator_monitor(app, monitors, names), monitors.len());
    match monitors.get(index) {
        Some(monitor) => (monitor_route(monitor, names), Some(index)),
        None => (OutputRoute { name: None, x: 0, y: 0, width: 0, height: 0, fullscreen: true, size: None, aspect: None, device: None }, None),
    }
}
pub(super) fn operator_monitor(app: &tauri::AppHandle, monitors: &[tauri::Monitor], names: &Names) -> Option<usize> {
    app.get_webview_window("main").and_then(|w| w.current_monitor().ok().flatten()).and_then(|m| matching_monitor(&monitor_route(&m, names), monitors, names))
}
