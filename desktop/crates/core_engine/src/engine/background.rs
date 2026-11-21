use protocol::{
    Background, BackgroundKind, EngineEvent, SessionItem, SessionItemKind,
};

use super::*;

impl Engine {
    pub(super) fn item_background(&self, item: &SessionItem) -> Option<Background> {
        match &item.kind {
            SessionItemKind::Text { .. } | SessionItemKind::Audio { .. } | SessionItemKind::Capture { .. } | SessionItemKind::Section {} => self.song_background.clone(),
            SessionItemKind::Image { path } => Some(Background { kind: BackgroundKind::Image, path: path.clone() }),
            SessionItemKind::Video { path, .. } => Some(Background { kind: BackgroundKind::Video, path: path.clone() }),
            SessionItemKind::Presentation { path, slides } => {
                let index = self.state.program_state.active_step_index.unwrap_or(0) as usize;
                Some(Background { kind: BackgroundKind::Image, path: slides.get(index).unwrap_or(path).clone() })
            }
        }
    }

    pub(super) fn show_background(
        &mut self,
        background: Option<Background>,
        ports: &mut Ports,
        errors: &mut Vec<EngineEvent>,
    ) {
        let Some(background) = background else {
            self.state.program_state.background = None;
            return;
        };
        match ports.assets.check_background(&background) {
            Ok(()) => self.state.program_state.background = Some(background),
            Err(e) => errors.push(error(
                "background",
                format!("Failed to load background {}: {e:#}", background.path),
            )),
        }
    }
}

pub(super) fn background_kind(path: &str) -> BackgroundKind {
    if path.starts_with(protocol::SHADER_PREFIX) {
        return BackgroundKind::Shader;
    }
    let lower = path.to_ascii_lowercase();
    if [".mp4", ".mkv", ".mov", ".webm"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        BackgroundKind::Video
    } else {
        BackgroundKind::Image
    }
}
