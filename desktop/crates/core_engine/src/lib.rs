//! The projection state machine: selection, display policy and media
//! sessions. It has no GPU, audio or decoder dependencies; adapters in the app
//! connect it to outputs, media and storage.

mod engine;
mod projector;
pub mod song;

pub use engine::{coalesce, Engine, EngineInput, MediaBackend, Output, Ports, ProjectStore};
pub use projector::Projector;

pub use protocol::{EngineCommand, EngineEvent, EngineSnapshot, ProgramState};
