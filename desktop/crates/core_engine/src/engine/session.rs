use protocol::{
    DisplayMode, MediaDirective, PlaybackKind, PlaybackPhase, PlaybackState, SessionItem, SessionItemKind, VideoOptions,
};

use super::*;

impl Engine {
    /// Keeps exactly one foreground session for the active media item. Edits to
    /// the active item's options reconfigure the session without restarting it.
    pub(super) fn sync_session(&mut self, item: Option<&SessionItem>, ports: &mut Ports) {
        let target = item.and_then(|item| match &item.kind {
            SessionItemKind::Video { path, options } => Some((item.id, PlaybackKind::Video, path, options)),
            SessionItemKind::Audio { path, options } => Some((item.id, PlaybackKind::Audio, path, options)),
            _ => None,
        });
        if let (Some(playback), Some((item_id, kind, path, options))) = (self.state.playback.as_mut(), target) {
            if playback.item_id == item_id && playback.kind == kind && &playback.path == path {
                let before = playback.clone();
                apply_options(playback, options);
                if *playback != before {
                    ports.media.send(MediaDirective::Configure { playback: playback.clone() });
                }
                return;
            }
        }
        if let Some(old) = self.state.playback.take() {
            ports.media.send(MediaDirective::Close { session_id: old.session_id });
        }
        if let Some((item_id, kind, path, options)) = target {
            let session_id = self.next_session_id;
            self.next_session_id += 1;
            let mut playback = PlaybackState {
                session_id,
                generation: 0,
                item_id,
                path: path.clone(),
                kind,
                paused: !self.autoplay,
                suspended: kind == PlaybackKind::Video && self.state.program_state.display == DisplayMode::Logo,
                phase: PlaybackPhase::Preparing,
                position_ms: options.start_ms,
                duration_ms: None,
                position_at_ms: ports.now_ms,
                volume: 100,
                muted: false,
                rate_percent: 100,
                looping: false,
                start_ms: 0,
                end_ms: None,
                confidence: false,
                error: None,
            };
            apply_options(&mut playback, options);
            ports.media.send(MediaDirective::Open { playback: playback.clone() });
            self.state.playback = Some(playback);
        }
    }
}

/// Starts a new seek generation at `position_ms`.
pub(super) fn seek(playback: &mut PlaybackState, position_ms: u64, ports: &mut Ports) {
    playback.generation += 1;
    playback.position_ms = position_ms;
    playback.position_at_ms = ports.now_ms;
    if playback.phase == PlaybackPhase::Ended {
        playback.phase = PlaybackPhase::Paused;
    }
    ports.media.send(MediaDirective::Seek {
        session_id: playback.session_id,
        generation: playback.generation,
        position_ms,
    });
}

pub(super) fn transport_directive(playback: &PlaybackState) -> MediaDirective {
    let session_id = playback.session_id;
    if playback.running() {
        MediaDirective::Play { session_id }
    } else {
        MediaDirective::Pause { session_id }
    }
}

/// Copies saved clip/volume/rate/loop options onto a session.
pub(super) fn apply_options(playback: &mut PlaybackState, options: &VideoOptions) {
    playback.volume = options.volume.min(100);
    playback.rate_percent = options.rate_percent.clamp(50, 200);
    playback.looping = options.looping;
    playback.confidence = options.confidence;
    playback.start_ms = options.start_ms;
    // Empty or reversed clips play to the end.
    playback.end_ms = options.end_ms.filter(|&end| end > options.start_ms);
}
