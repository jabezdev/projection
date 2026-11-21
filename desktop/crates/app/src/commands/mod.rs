//! Operator UI commands: engine control, the song and service library, and
//! media files. Output and display commands live in `displays`, settings in
//! `settings`.

use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use core_engine::{EngineCommand, EngineSnapshot};
use protocol::{ProjectHeader, ProjectModel, SongHeader, SongModel};

use crate::{now_ms, AppState};

mod media;
mod missing_media;
mod pdf;
mod services;
mod songs;
mod system;
mod transport;

pub use media::*;
pub use missing_media::*;
pub use pdf::*;
pub use services::*;
pub use songs::*;
pub use system::*;
pub use transport::*;
