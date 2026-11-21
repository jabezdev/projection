//! The application's single audio output and the per-session sample feed.

mod convert;
mod output;
mod session;

pub use output::{audio_recoveries, opened_outputs, AudioOutput, DeviceFormat};
pub use session::SessionAudio;
use convert::feeder;
use output::Slot;

#[cfg(test)]
mod tests;
