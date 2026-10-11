//! The projection state machine: selection, display policy and media
//! sessions. It has no GPU, audio or decoder dependencies; adapters in the app
//! connect it to outputs, media and storage.

mod engine;
mod assets;
pub mod song;

pub use assets::Files;
pub use engine::{coalesce, Assets, Engine, EngineInput, MediaBackend, Ports, ProjectStore};

pub use protocol::{EngineCommand, EngineEvent, EngineSnapshot, ProgramState};
