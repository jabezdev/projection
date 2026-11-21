use std::collections::BTreeSet;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

mod commands;
mod matching;
mod placement;

pub use commands::*;
pub use placement::*;
use matching::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputRoute {
    name: Option<String>, x: i32, y: i32, width: u32, height: u32,
    #[serde(default = "yes")] fullscreen: bool,
    #[serde(default)] size: Option<(u32, u32)>,
    #[serde(default)] aspect: Option<(u32, u32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")] device: Option<String>,
}
fn yes() -> bool { true }
pub const OUTPUTS: [(&str, &str); 2] = [("projection", "PALTO Projection"), ("confidence", "PALTO Confidence")];
pub const WINDOWED_SIZE: (u32, u32) = (1280, 720);
pub struct OutputRoutes {
    pub values: Mutex<std::collections::BTreeMap<String, OutputRoute>>,
    pub path: std::path::PathBuf,
    pub shown: Mutex<BTreeSet<String>>,
}
impl OutputRoutes {
    pub fn new(values: std::collections::BTreeMap<String, OutputRoute>, path: std::path::PathBuf) -> Self {
        Self { values: Mutex::new(values), path, shown: Mutex::default() }
    }
    pub fn is_shown(&self, output: &str) -> bool {
        self.shown.lock().is_ok_and(|shown| shown.contains(output))
    }
    pub fn any_shown(&self) -> bool {
        self.shown.lock().is_ok_and(|shown| !shown.is_empty())
    }
    fn set_shown(&self, output: &str, shown: bool) {
        if let Ok(mut set) = self.shown.lock() {
            if shown { set.insert(output.to_string()); } else { set.remove(output); }
        }
    }
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn wants_fullscreen(&self, output: &str) -> bool {
        self.values.lock().ok().is_some_and(|saved| saved.get(output).is_none_or(|route| route.fullscreen))
    }
}

#[cfg(test)]
mod tests;
