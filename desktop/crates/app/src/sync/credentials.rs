use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE: &str = "sync.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    #[serde(default)]
    pub offline: bool,
}

impl Credentials {
    pub fn paired(&self) -> Option<(&str, &str)> {
        Some((self.deployment.as_deref()?, self.token.as_deref()?))
    }

    pub fn load(dir: &Path) -> Credentials {
        std::fs::read_to_string(path(dir)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let partial = dir.join(format!("{FILE}.partial"));
        std::fs::write(&partial, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(partial, path(dir))
    }
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}
