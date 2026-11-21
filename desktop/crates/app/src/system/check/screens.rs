use tauri::Manager;

use super::report::Report;
use crate::output::display_names::{display_names, readable_name, refresh_rate};
use crate::settings::OutputState;

pub fn check(report: &mut Report, app: &tauri::AppHandle, fonts: &render_engine::Fonts) {
    report.section("Displays");
    let monitors = match app.get_webview_window("main").map(|w| w.available_monitors()) {
        Some(Ok(monitors)) => monitors,
        Some(Err(e)) => return report.fail(format!("Cannot list the displays: {e}")),
        None => return report.fail("The control window is not open"),
    };
    let names = display_names();
    for (index, monitor) in monitors.iter().enumerate() {
        let system = monitor.name().map(String::as_str);
        let name = readable_name(system, system.and_then(|n| names.get(n)));
        let size = monitor.size();
        let hz = system.and_then(refresh_rate).map(|hz| format!(", {hz} Hz")).unwrap_or_default();
        let scale = monitor.scale_factor();
        let line = format!("{}. {name}: {}×{}{hz}, scaling {:.0} %", index + 1, size.width, size.height, scale * 100.0);
        if scale > 1.0 { report.info(format!("{line} (outputs still draw every physical pixel)")) } else { report.ok(line) }
    }
    match monitors.len() {
        0 => report.fail("No displays found"),
        1 => report.warn("Only one display is connected; connect the projector and confidence screen to test the outputs"),
        n => report.ok(format!("{n} displays connected")),
    }

    report.section("Fonts");
    let (font, confidence) = {
        let config = app.state::<OutputState>();
        let config = config.config.lock().map(|c| (c.font.clone(), c.confidence_font.clone()));
        config.unwrap_or_default()
    };
    for (role, family) in [("Projection", font), ("Confidence", confidence)] {
        let generic = matches!(family.trim().to_ascii_lowercase().as_str(), "" | "sans-serif" | "serif" | "monospace");
        if generic || fonts.has_family(&family) {
            report.ok(format!("{role} font \"{family}\" is installed"));
        } else {
            report.warn(format!("{role} font \"{family}\" is not installed; a substitute is used"));
        }
    }
}
