use protocol::{
    CaptureFit, PlaybackState, ProgramState, ProjectModel,
    SessionItem, SessionItemKind,
};
use render_engine::VideoKey;

mod confidence;
mod feed;
mod media_plan;
mod projection;
mod style;

pub use confidence::*;
pub use feed::*;
pub use media_plan::*;
pub use projection::*;
pub use style::*;

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "mkv", "webm", "avi"];
const NON_SONG_COLOR: &str = "#7d9baf";

fn is_video(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIONS.iter().any(|v| v.eq_ignore_ascii_case(e)))
}

fn active_item<'a>(program: &ProgramState, project: Option<&'a ProjectModel>) -> Option<&'a SessionItem> {
    let id = program.active_item_id?;
    project?.session.items.iter().find(|item| item.id == id)
}

fn foreground_video(item: Option<&SessionItem>, playback: Option<&PlaybackState>) -> Option<VideoKey> {
    let item = item?;
    let playback = playback?;
    match &item.kind {
        SessionItemKind::Video { path, .. } if playback.item_id == item.id && playback.path == *path => {
            Some(VideoKey::Session(playback.session_id))
        }
        _ => None,
    }
}

fn capture_on(item: Option<&SessionItem>, confidence: bool) -> Option<(VideoKey, bool)> {
    let SessionItemKind::Capture { source, options } = &item?.kind else { return None };
    let (shown, fit) = if confidence { (options.confidence, options.confidence_fit) } else { (options.projection, options.projection_fit) };
    shown.then(|| (VideoKey::Capture(source.id.clone()), fit == CaptureFit::Fill))
}

#[cfg(test)]
mod tests;
