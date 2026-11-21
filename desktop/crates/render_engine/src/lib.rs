mod engine;
mod frame_pump;
mod gpu;

mod capture;
mod confidence;
mod frame;
mod main_scene;
mod media;
mod monitor;
mod output;
mod paint;
mod preview;
pub mod profile;
pub mod scene;
mod shader_bg;
mod test_source;
pub mod text;
mod yuv;

pub use capture::Capture;
pub use frame::{FramePixels, PixelFrame, SharedFrame, VideoFrames, VideoKey};
pub use main_scene::LyricGeometry;
pub use media::MediaTextures;
pub use monitor::MonitorSpec;
pub use output::{OutputRole, OutputStats, StatusSink};
pub use paint::{css_color, Rgba, TextEffects};
pub use preview::{PreviewTarget, Snapshot, SnapshotSink};
pub use scene::{
    ConfidenceGroup, ConfidenceScene, ConfidenceStyle, Direction, Display, Easing, LyricStyle, MainScene, MediaLayer, MediaSource, Scene, Shadow, Stroke,
    Transition, TransitionKind, Vertical,
};
pub use shader_bg::{ShaderSpec, SHADER_SIZE};
pub use test_source::TestPatternSource;
pub use text::{Fonts, TextAlign};
pub use yuv::{YuvLayout, YuvMatrix, YuvPixels};

use monitor::MonitorWorker;
use output::{OutputWorker, Resources};
use preview::Previews;

pub use engine::{fit_aspect, OutputSpec, RenderEngine, FRAME_CAPACITY};
pub use gpu::Gpu;
use frame_pump::FramePump;
use gpu::new_instance;
