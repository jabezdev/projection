//! Decodes real media generated with the FFmpeg CLI. A test thread plays the
//! role of the audio device, consuming samples at real-time pace.



mod ambient;
mod formats;
mod support;
mod transport;

use support::*;
