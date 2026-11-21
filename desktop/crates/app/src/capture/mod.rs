//! Screen and window capture (roadmap phase 8). A capture item's source is
//! captured once while it is on air and shown on the screens it is routed
//! to, like a video: frames go to the renderer's shared texture as BGRA and
//! are scaled there. The method is chosen per platform (detected, with a
//! default and a setting to switch): Windows.Graphics.Capture or Desktop
//! Duplication on Windows; the PipeWire screen-cast portal or X11 on Linux.
//!
//! A source that disappears (window closed, display unplugged, sharing
//! stopped) is replaced by black at once, never left frozen on air, and the
//! operator is told.

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

/// A running capture; dropping it stops capturing.
pub trait CaptureStream: Send {}

/// One capture method.
pub trait Backend: Send + Sync {
    /// `wgc`, `dxgi`, `portal` or `x11`.
    fn name(&self) -> &'static str;
    fn label(&self) -> &'static str;
    /// The system shows its own picker (the portal) instead of a list.
    fn system_picker(&self) -> bool {
        false
    }
    /// Screens and windows to choose from.
    fn sources(&self) -> anyhow::Result<Vec<CaptureSource>>;
    /// Asks the system to pick a source (only with [`Backend::system_picker`]).
    fn pick(&self, _kind: CaptureKind) -> anyhow::Result<Option<CaptureSource>> {
        anyhow::bail!("{} lists its sources instead", self.label())
    }
    fn start(&self, source: &CaptureSource, slot: FrameSlot) -> anyhow::Result<Box<dyn CaptureStream>>;
}

/// Capture methods that work here, best first.
#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
fn detect(data_dir: Option<&std::path::Path>) -> Vec<Arc<dyn Backend>> {
    let mut backends: Vec<Arc<dyn Backend>> = Vec::new();
    // Test aid: a moving pattern instead of a real screen.
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
