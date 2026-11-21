mod convert;
mod output;
mod session;

pub use output::{audio_recoveries, describe_devices, opened_outputs, AudioOutput, DeviceFormat};
pub use session::SessionAudio;
use convert::feeder;
use output::Slot;

#[cfg(test)]
mod tests;
