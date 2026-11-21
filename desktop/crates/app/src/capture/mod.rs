use std::sync::Arc;

use protocol::{CaptureKind, CaptureSource};

#[cfg(target_os = "linux")]
mod portal;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
mod x11;
mod test_pattern;

mod captures;
mod commands;
mod frame_slot;

pub use captures::*;
pub use commands::*;
pub use frame_slot::*;

pub trait CaptureStream: Send {}

pub trait Backend: Send + Sync {
    fn name(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn system_picker(&self) -> bool {
        false
    }
    fn sources(&self) -> anyhow::Result<Vec<CaptureSource>>;
    fn pick(&self, _kind: CaptureKind) -> anyhow::Result<Option<CaptureSource>> {
        anyhow::bail!("{} lists its sources instead", self.label())
    }
    fn start(&self, source: &CaptureSource, slot: FrameSlot) -> anyhow::Result<Box<dyn CaptureStream>>;
}

#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
fn detect(data_dir: Option<&std::path::Path>) -> Vec<Arc<dyn Backend>> {
    let mut backends: Vec<Arc<dyn Backend>> = Vec::new();
    if std::env::var("MAGI_TEST_CAPTURE").as_deref() == Ok("1") {
        backends.push(Arc::new(test_pattern::TestPattern));
    }
    #[cfg(windows)]
    {
        if windows::wgc_supported() {
            backends.push(Arc::new(windows::Wgc));
        }
        backends.push(Arc::new(windows::Dxgi));
    }
    #[cfg(target_os = "linux")]
    {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland");
        if wayland {
            backends.push(Arc::new(portal::Portal::new(data_dir)));
        }
        if std::env::var_os("DISPLAY").is_some() {
            backends.push(Arc::new(x11::X11));
        }
        if !wayland {
            backends.push(Arc::new(portal::Portal::new(data_dir)));
        }
    }
    backends
}

#[cfg(test)]
mod tests;
