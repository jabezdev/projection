//! Pairing remote devices. The operator starts pairing, which shows a
//! six-digit code for a few minutes; a device that sends the code gets a
//! random token it presents on every connection. Only a hash of each token
//! is kept, in `remote-devices.json`. Wrong codes are limited: after a few,
//! the code is withdrawn and the operator starts again.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const FILENAME: &str = "remote-devices.json";
/// How long a pairing code stays valid.
pub const CODE_LIFETIME: Duration = Duration::from_secs(5 * 60);
/// Wrong codes allowed within [`FAILURE_WINDOW`] before the code is withdrawn.
const MAX_FAILURES: usize = 5;
const FAILURE_WINDOW: Duration = Duration::from_secs(60);
/// `last_seen` is written to disk at most this often per device.
const SEEN_SAVE_INTERVAL_MS: i64 = 60_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    token_sha256: String,
    pub paired_ms: i64,
    pub last_seen_ms: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    devices: Vec<Device>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairError {
    /// No code is being shown on the operator screen.
    NotPairing,
    WrongCode,
    /// Too many wrong codes; the operator must start pairing again.
    Locked,
    Storage(String),
}

impl std::fmt::Display for PairError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PairError::NotPairing => write!(f, "Pairing is not open. On the PALTO computer, open Settings → Remote and choose Pair a device."),
            PairError::WrongCode => write!(f, "That code is not right. Check the code on the PALTO computer."),
            PairError::Locked => write!(f, "Too many wrong codes. On the PALTO computer, start pairing again."),
            PairError::Storage(e) => write!(f, "The device could not be saved: {e}"),
        }
    }
}

struct Inner {
    code: Option<(String, Instant)>,
    failures: VecDeque<Instant>,
    devices: Vec<Device>,
    /// Bumped whenever a device is removed, so connections re-check.
    revision: u64,
}

pub struct Pairing {
    inner: Mutex<Inner>,
    path: Option<PathBuf>,
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("the system random source is available");
    bytes
}

fn hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

impl Pairing {
    /// Loads paired devices from `path` (none when it is missing or unreadable).
    pub fn load(path: PathBuf) -> Self {
        let saved: Saved = std::fs::read(&path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        Self { inner: Mutex::new(Inner { code: None, failures: VecDeque::new(), devices: saved.devices, revision: 0 }), path: Some(path) }
    }

    /// Without a file, for tests.
    pub fn in_memory() -> Self {
        Self { inner: Mutex::new(Inner { code: None, failures: VecDeque::new(), devices: Vec::new(), revision: 0 }), path: None }
    }

    fn save(&self, inner: &Inner) -> Result<(), String> {
        let Some(path) = &self.path else { return Ok(()) };
        let bytes = serde_json::to_vec_pretty(&Saved { devices: inner.devices.clone() }).map_err(|e| e.to_string())?;
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, path)).map_err(|e| e.to_string())
    }

    /// Shows a new code for [`CODE_LIFETIME`]; returns it.
    pub fn start(&self, now: Instant) -> String {
        let mut inner = self.inner.lock().expect("pairing poisoned");
        let code = format!("{:06}", u32::from_le_bytes(random_bytes()) % 1_000_000);
        inner.code = Some((code.clone(), now + CODE_LIFETIME));
        inner.failures.clear();
        code
    }

    pub fn stop(&self) {
        self.inner.lock().expect("pairing poisoned").code = None;
    }

    /// The code being shown and how long it stays valid.
    pub fn code(&self, now: Instant) -> Option<(String, Duration)> {
        let mut inner = self.inner.lock().expect("pairing poisoned");
        match &inner.code {
            Some((code, until)) if *until > now => Some((code.clone(), *until - now)),
            Some(_) => {
                inner.code = None;
                None
            }
            None => None,
        }
    }

    /// Pairs a device that sent `code`; returns its token.
    pub fn pair(&self, code: &str, name: &str, now: Instant, now_ms: i64) -> Result<String, PairError> {
        let mut inner = self.inner.lock().expect("pairing poisoned");
        let current = match &inner.code {
            Some((current, until)) if *until > now => current.clone(),
            _ => return Err(PairError::NotPairing),
        };
        if code.trim() != current {
            while inner.failures.front().is_some_and(|at| now.duration_since(*at) > FAILURE_WINDOW) {
                inner.failures.pop_front();
            }
            inner.failures.push_back(now);
            if inner.failures.len() >= MAX_FAILURES {
                inner.code = None;
                log::warn!("Remote pairing locked after {MAX_FAILURES} wrong codes");
                return Err(PairError::Locked);
            }
            return Err(PairError::WrongCode);
        }
        let token = hex::encode(random_bytes::<32>());
        let name: String = name.trim().chars().take(60).collect();
        let device = Device {
            id: hex::encode(random_bytes::<8>()),
            name: if name.is_empty() { "Remote device".into() } else { name },
            token_sha256: hash(&token),
            paired_ms: now_ms,
            last_seen_ms: now_ms,
        };
        log::info!("Paired remote device \"{}\"", device.name);
        inner.devices.push(device);
        self.save(&inner).map_err(PairError::Storage)?;
        Ok(token)
    }

    /// The paired device presenting `token`, noting that it was seen.
    pub fn authorize(&self, token: &str, now_ms: i64) -> Option<Device> {
        let digest = hash(token);
        let mut inner = self.inner.lock().expect("pairing poisoned");
        let device = inner.devices.iter_mut().find(|d| d.token_sha256 == digest)?;
        let save = now_ms - device.last_seen_ms >= SEEN_SAVE_INTERVAL_MS;
        device.last_seen_ms = now_ms;
        let found = device.clone();
        if save {
            if let Err(e) = self.save(&inner) {
                log::warn!("Saving remote devices: {e}");
            }
        }
        Some(found)
    }

    /// Whether device `id` is still paired.
    pub fn is_paired(&self, id: &str) -> bool {
        self.inner.lock().expect("pairing poisoned").devices.iter().any(|d| d.id == id)
    }

    pub fn devices(&self) -> Vec<Device> {
        self.inner.lock().expect("pairing poisoned").devices.clone()
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().expect("pairing poisoned");
        inner.devices.retain(|d| d.id != id);
        inner.revision += 1;
        self.save(&inner)
    }

    /// Changes when a device is removed.
    pub fn revision(&self) -> u64 {
        self.inner.lock().expect("pairing poisoned").revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_pairs_with_the_shown_code_and_is_known_by_its_token() {
        let pairing = Pairing::in_memory();
        let now = Instant::now();
        assert_eq!(pairing.pair("123456", "Phone", now, 0), Err(PairError::NotPairing));
        let code = pairing.start(now);
        assert_eq!(code.len(), 6);
        let token = pairing.pair(&code, " Leader's phone ", now, 1_000).unwrap();
        let device = pairing.authorize(&token, 2_000).expect("the token is known");
        assert_eq!(device.name, "Leader's phone");
        assert!(pairing.authorize("not-a-token", 2_000).is_none());
        pairing.remove(&device.id).unwrap();
        assert!(pairing.authorize(&token, 3_000).is_none(), "a removed device is refused");
    }

    #[test]
    fn codes_expire_and_wrong_codes_lock_pairing() {
        let pairing = Pairing::in_memory();
        let now = Instant::now();
        let code = pairing.start(now);
        assert_eq!(pairing.pair(&code, "Late", now + CODE_LIFETIME + Duration::from_secs(1), 0), Err(PairError::NotPairing));
        let code = pairing.start(now);
        let wrong = if code == "000000" { "111111" } else { "000000" };
        for _ in 0..MAX_FAILURES - 1 {
            assert_eq!(pairing.pair(wrong, "Guess", now, 0), Err(PairError::WrongCode));
        }
        assert_eq!(pairing.pair(wrong, "Guess", now, 0), Err(PairError::Locked));
        assert_eq!(pairing.pair(&code, "Phone", now, 0), Err(PairError::NotPairing), "the code was withdrawn");
    }
}
