use std::path::{Path, PathBuf};
use std::time::Duration;

use media_engine::bench::{decode_run, luma_difference, DecodeRun};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use super::report::Report;

const VIDEOS: usize = 3;
const MAX_FRAMES: u32 = 300;
const MAX_WALL: Duration = Duration::from_secs(8);
const EXTENSIONS: [&str; 6] = ["mp4", "mov", "m4v", "mkv", "webm", "avi"];
const MIN_REALTIME: f64 = 1.5;
const MAX_LUMA_DIFFERENCE: f64 = 2.0;
const CPU_SAVING: f64 = 0.6;

#[derive(Debug, Clone)]
pub struct Measured {
    pub run: DecodeRun,
    pub cpu: Duration,
}

pub struct Trial {
    pub video: String,
    pub software: Result<Measured, String>,
    pub hardware: Vec<(&'static str, Result<Measured, String>)>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Recommendation {
    pub choice: String,
    pub reason: String,
}

pub fn library_videos(data_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(data_dir.join("media")) else { return Vec::new() };
    let mut videos: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str())))
        .filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p)))
        .collect();
    videos.sort_by(|a, b| b.0.cmp(&a.0));
    videos.into_iter().take(VIDEOS).map(|(_, p)| p).collect()
}

fn cpu_time(system: &mut System, pid: Pid) -> Duration {
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), false, ProcessRefreshKind::nothing().with_cpu());
    Duration::from_millis(system.process(pid).map_or(0, |p| p.accumulated_cpu_time()))
}

pub fn run_trials(videos: &[PathBuf], stop: &dyn Fn() -> bool) -> Vec<Trial> {
    let devices = media_engine::hwaccel::available();
    let mut system = System::new().ok();
    let pid = sysinfo::get_current_pid().ok();
    let mut measure = |path: &str, device: Option<&'static str>| -> Result<Measured, String> {
        let before = system.as_mut().zip(pid).map(|(s, pid)| cpu_time(s, pid));
        let run = decode_run(path, device, MAX_FRAMES, MAX_WALL, stop).map_err(|e| format!("{e:#}"))?;
        let after = system.as_mut().zip(pid).map(|(s, pid)| cpu_time(s, pid));
        Ok(Measured { run, cpu: after.zip(before).map_or(Duration::ZERO, |(a, b)| a.saturating_sub(b)) })
    };
    let mut trials = Vec::new();
    for video in videos {
        if stop() {
            break;
        }
        let path = video.to_string_lossy();
        let software = measure(&path, None);
        let hardware = devices.iter().take_while(|_| !stop()).map(|(name, _)| (*name, measure(&path, Some(name)))).collect();
        trials.push(Trial { video: video.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), software, hardware });
    }
    trials
}

fn qualifies(software: &Measured, hardware: &Measured) -> bool {
    hardware.run.frames + 2 >= software.run.frames
        && hardware.run.realtime_factor() >= MIN_REALTIME
        && luma_difference(&software.run, &hardware.run).is_none_or(|d| d <= MAX_LUMA_DIFFERENCE)
}

pub fn recommend(trials: &[Trial]) -> Option<Recommendation> {
    let tested: Vec<(&Measured, &Trial)> = trials.iter().filter_map(|t| t.software.as_ref().ok().map(|s| (s, t))).collect();
    if tested.is_empty() {
        return None;
    }
    let software_cpu: Duration = tested.iter().map(|(s, _)| s.cpu).sum();
    let software_slow = tested.iter().any(|(s, _)| s.run.realtime_factor() < MIN_REALTIME);
    let devices: Vec<&'static str> = tested[0].1.hardware.iter().map(|(name, _)| *name).collect();
    let best = devices
        .into_iter()
        .filter_map(|device| {
            let runs: Option<Vec<&Measured>> = tested
                .iter()
                .map(|(software, trial)| {
                    trial.hardware.iter().find(|(n, _)| *n == device).and_then(|(_, r)| r.as_ref().ok()).filter(|hw| qualifies(software, hw))
                })
                .collect();
            runs.map(|runs| (device, runs.iter().map(|r| r.cpu).sum::<Duration>()))
        })
        .min_by_key(|(_, cpu)| *cpu);
    let saving = |cpu: Duration| 100.0 * (1.0 - cpu.as_secs_f64() / software_cpu.as_secs_f64().max(1e-6));
    Some(match best {
        Some((device, _)) if software_slow => Recommendation { choice: device.into(), reason: "software decoding cannot keep up with these videos".into() },
        Some((device, cpu)) if cpu.as_secs_f64() <= software_cpu.as_secs_f64() * CPU_SAVING => {
            Recommendation { choice: device.into(), reason: format!("it decodes correctly and uses {:.0} % less processor time", saving(cpu)) }
        }
        Some((device, cpu)) => Recommendation {
            choice: "software".into(),
            reason: format!("{device} works but saves only {:.0} % processor time", saving(cpu).max(0.0)),
        },
        None if software_slow => Recommendation { choice: "software".into(), reason: "no hardware decoder handled these videos correctly; software is slow, so use smaller videos".into() },
        None => Recommendation { choice: "software".into(), reason: "no hardware decoder handled these videos correctly".into() },
    })
}

pub fn describe(report: &mut Report, trials: &[Trial]) {
    for trial in trials {
        let software = trial.software.as_ref();
        match software {
            Ok(s) => report.info(format!(
                "{}: {} {}×{} {}, software {:.0} fps ({:.1}× real time, {} ms CPU)",
                trial.video, s.run.codec, s.run.size.0, s.run.size.1, s.run.pixel_format, s.run.fps(), s.run.realtime_factor(), s.cpu.as_millis()
            )),
            Err(e) => report.warn(format!("{}: software decoding failed: {e}", trial.video)),
        }
        for (device, result) in &trial.hardware {
            match (result, software) {
                (Ok(hw), Ok(s)) => {
                    let difference = luma_difference(&s.run, &hw.run).map_or("not compared".into(), |d| format!("{d:.2}"));
                    report.info(format!(
                        "{}: {device} {:.0} fps ({:.1}× real time, {} ms CPU), picture difference {difference}",
                        trial.video, hw.run.fps(), hw.run.realtime_factor(), hw.cpu.as_millis()
                    ));
                }
                (Ok(hw), Err(_)) => report.info(format!("{}: {device} {:.0} fps", trial.video, hw.run.fps())),
                (Err(e), _) => report.info(format!("{}: {device} unavailable: {e}", trial.video)),
            }
        }
    }
}

#[cfg(test)]
mod tests;
