use std::path::Path;
use std::time::{Duration, SystemTime};

use sysinfo::{CpuRefreshKind, Disks, System};

use super::report::Report;
use crate::system::backup;

const GB: f64 = 1024.0 * 1024.0 * 1024.0;

pub fn check(report: &mut Report, data_dir: &Path) {
    report.section("Computer");
    let os = System::long_os_version().unwrap_or_else(|_| "unknown system".into());
    report.info(format!("{os} ({})", System::cpu_arch()));
    match System::new() {
        Ok(mut system) => {
            system.refresh_cpu_list(CpuRefreshKind::nothing());
            system.refresh_memory();
            let brand = system.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
            let cores = System::physical_core_count().map(|n| n.to_string()).unwrap_or_else(|_| "?".into());
            report.info(format!("{brand}, {cores} cores, {} threads", system.cpus().len()));
            let (total, free) = (system.total_memory() as f64 / GB, system.available_memory() as f64 / GB);
            let memory = format!("{total:.1} GB memory, {free:.1} GB free");
            if total < 7.5 || free < 1.5 { report.warn(memory) } else { report.ok(memory) }
        }
        Err(e) => report.warn(format!("Cannot read the processor and memory: {e}")),
    }
    report.section("Storage");
    report.info(format!("Data folder: {}", data_dir.display()));
    let probe = data_dir.join(".system-check");
    match std::fs::write(&probe, b"ok").and_then(|_| std::fs::remove_file(&probe)) {
        Ok(()) => report.ok("The data folder is writable"),
        Err(e) => report.fail(format!("The data folder is not writable: {e}")),
    }
    match free_space(data_dir) {
        Some(free) if free < 5.0 => report.warn(format!("Only {free:.1} GB free on the data drive")),
        Some(free) => report.ok(format!("{free:.0} GB free on the data drive")),
        None => report.info("Free space on the data drive is unknown"),
    }
    let age = backup::last_automatic(data_dir).and_then(|t| SystemTime::now().duration_since(t).ok());
    match age {
        Some(age) if age > Duration::from_secs(8 * 86_400) => report.warn(format!("The last automatic backup is {} days old", age.as_secs() / 86_400)),
        Some(age) => report.ok(format!("Last automatic backup {} days ago", age.as_secs() / 86_400)),
        None => report.info("No automatic backup yet; the first is made a minute after start"),
    }
}

fn free_space(dir: &Path) -> Option<f64> {
    let dir = dir.canonicalize().ok()?;
    let disks = Disks::new_with_refreshed_list().ok()?;
    disks
        .iter()
        .filter(|disk| dir.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| disk.available_space() as f64 / GB)
}
