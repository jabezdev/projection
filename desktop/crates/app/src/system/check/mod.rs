mod decoding;
mod devices;
mod graphics;
mod machine;
mod report;
mod screens;

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::Manager;

use crate::output::displays::OutputRoutes;
use crate::settings::ConfigStore;
use crate::system::logging::logs_dir;
use report::Report;

const START_AFTER: Duration = Duration::from_secs(15);
const FORCE_FLAG: &str = "--system-check";
const LOG_FILE: &str = "system-check.log";
const PREVIOUS_LOG_FILE: &str = "system-check.previous.log";
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn start_if_due(app: &tauri::AppHandle, data_dir: &Path) {
    let forced = std::env::args().any(|a| a == FORCE_FLAG);
    let last = app.state::<ConfigStore>().get().system_check_version;
    if !forced && last == VERSION {
        return;
    }
    let reason = if forced { format!("started with {FORCE_FLAG}") } else if last.is_empty() { "first start".into() } else { format!("updated from {last}") };
    let (app, data_dir) = (app.clone(), data_dir.to_path_buf());
    let spawned = std::thread::Builder::new().name("system-check".into()).spawn(move || {
        std::thread::sleep(START_AFTER);
        run(&app, &data_dir, &reason);
    });
    if let Err(e) = spawned {
        log::error!("Starting the system check: {e}");
    }
}

fn run(app: &tauri::AppHandle, data_dir: &Path, reason: &str) {
    log::info!("System check started ({reason})");
    let live = || app.state::<OutputRoutes>().any_shown();
    let mut report = Report::default();
    machine::check(&mut report, data_dir);
    let fonts = render_engine::Fonts::load();
    screens::check(&mut report, app, &fonts);
    drop(fonts);
    devices::audio(&mut report);
    devices::controls(&mut report, app);
    devices::network(&mut report, app);
    let mut finished = !live();
    if finished {
        graphics::check(&mut report);
        finished = !live() && decoder(&mut report, app, data_dir, &live);
    }
    if !finished {
        report.section("Not finished");
        report.info("The outputs were shown, so the performance tests stopped. They run again at the next start.");
    }
    let path = write(&report, reason);
    let problems = report.problems().len();
    match &path {
        Ok(path) => log::info!("System check: {problems} item(s) need attention; see {}", path.display()),
        Err(e) => log::error!("Writing the system check: {e}"),
    }
    if finished {
        if let Err(e) = app.state::<ConfigStore>().update(|c| c.system_check_version = VERSION.into()) {
            log::warn!("Saving the system check version: {e}");
        }
    }
}

fn decoder(report: &mut Report, app: &tauri::AppHandle, data_dir: &Path, live: &dyn Fn() -> bool) -> bool {
    report.section("Video decoding");
    let videos = decoding::library_videos(data_dir);
    if videos.is_empty() {
        report.info(format!("No videos in the library to test. Import one and start PALTO with {FORCE_FLAG} to choose the decoder."));
        return true;
    }
    let trials = decoding::run_trials(&videos, live);
    if live() {
        return false;
    }
    decoding::describe(report, &trials);
    let Some(pick) = decoding::recommend(&trials) else {
        report.warn("None of the library videos could be decoded");
        return true;
    };
    let config = app.state::<ConfigStore>();
    let current = config.get();
    if current.decoder_chosen {
        report.info(format!("Recommended decoder: {} ({}); kept \"{}\", chosen in Settings", pick.choice, pick.reason, current.video_decoder));
        return true;
    }
    match config.update(|c| c.video_decoder = pick.choice.clone()) {
        Ok(_) => {
            media_engine::hwaccel::set_choice(media_engine::hwaccel::DecoderChoice::parse(&pick.choice));
            report.ok(format!("Decoder set to {}: {}", pick.choice, pick.reason));
        }
        Err(e) => report.fail(format!("Saving the decoder choice: {e}")),
    }
    true
}

fn write(report: &Report, reason: &str) -> std::io::Result<PathBuf> {
    let dir = logs_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(LOG_FILE);
    if path.exists() {
        let _ = std::fs::rename(&path, dir.join(PREVIOUS_LOG_FILE));
    }
    let header = [
        "PALTO system check".to_string(),
        format!("Version {VERSION}, {}", chrono::Local::now().format("%Y-%m-%d %H:%M")),
        format!("Why: {reason}"),
    ];
    std::fs::write(&path, report.render(&header))?;
    Ok(path)
}
