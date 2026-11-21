use protocol::{
    Background, DisplayMode, EngineCommand, EngineEvent,
    MediaAction, MediaDirective, LiveService, PlaybackKind, PlaybackPhase, SessionId,
};

use super::*;

impl Engine {
    pub(super) fn apply(&mut self, cmd: EngineCommand, ports: &mut Ports, errors: &mut Vec<EngineEvent>) -> bool {
        let program = &mut self.state.program_state;
        match cmd {
            EngineCommand::SetAutoplay { enabled } => {
                self.autoplay = enabled;
                return false;
            }
            EngineCommand::SetDisplay { display } => {
                let was_logo = program.display == DisplayMode::Logo;
                program.display = display;
                if let Some(playback) = self.state.playback.as_mut().filter(|p| p.kind == PlaybackKind::Video) {
                    let is_logo = display == DisplayMode::Logo;
                    if is_logo != was_logo {
                        let was_running = playback.running();
                        playback.suspended = is_logo;
                        if was_running != playback.running() {
                            ports.media.send(transport_directive(playback));
                        }
                    }
                }
            }
            EngineCommand::Media { session_id, action } => self.media(session_id, action, ports, errors),
            EngineCommand::Play | EngineCommand::Pause | EngineCommand::Stop | EngineCommand::Seek { .. } => {
                unreachable!("normalized to EngineCommand::Media")
            }
            EngineCommand::SetActiveText { text } => {
                program.active_step_index = None;
                program.active_text = text;
            }
            EngineCommand::SetBackground { path } => {
                let background = Background { kind: background_kind(&path), path };
                self.show_background(Some(background), ports, errors);
            }

            EngineCommand::LoadService { service_id } => match ports.store.load_service(&service_id) {
                Ok(service) => {
                    self.state.service = Some(service);
                    self.song_background = None;
                    self.select(None, None, true, ports, errors);
                    return true;
                }
                Err(e) => errors.push(error("service", format!("Failed to load service {service_id}: {e:#}"))),
            },
            EngineCommand::SaveService { service } => {
                let saved = ports.store.save_service(&service);
                return self.replace_service(saved, &service.id, ports, errors);
            }
            EngineCommand::RefreshService => {
                let Some(current) = self.state.service.as_ref().map(|s| s.service.clone()) else { return false };
                let resolved = ports.store.resolve_service(&current);
                return self.replace_service(resolved, &current.id, ports, errors);
            }

            nav @ (EngineCommand::Next
            | EngineCommand::Prev
            | EngineCommand::NextItem
            | EngineCommand::PrevItem
            | EngineCommand::Cue { .. }
            | EngineCommand::GoToItem { .. }
            | EngineCommand::NextStep
            | EngineCommand::PrevStep
            | EngineCommand::GoToStep { .. }) => {
                let Some(service) = &self.state.service else {
                    errors.push(error("navigation", "No service loaded; navigation has no effect."));
                    return false;
                };
                let (item, step) = navigation_target(service, &self.state.program_state, &nav);
                self.select(item, step, false, ports, errors);
            }
        }
        false
    }

    fn replace_service(&mut self, resolved: anyhow::Result<LiveService>, id: &str, ports: &mut Ports, errors: &mut Vec<EngineEvent>) -> bool {
        let live = match resolved {
            Ok(live) => live,
            Err(e) => {
                errors.push(error("service", format!("Failed to save service {id}: {e:#}")));
                return false;
            }
        };
        if self.state.service.as_ref().map(|s| s.id()) != Some(id) {
            return false;
        }
        let active_id = self.state.program_state.active_item_id.clone();
        let find = |s: &LiveService| active_id.as_deref().and_then(|id| s.item(id)).cloned();
        let old_item = self.state.service.as_ref().and_then(find);
        let new_item = find(&live);
        let media_changed = old_item.as_ref().map(|i| (&i.background, &i.kind)) != new_item.as_ref().map(|i| (&i.background, &i.kind));
        self.state.service = Some(live);
        let program = &self.state.program_state;
        let (item, step) = (program.active_item_id.clone(), program.active_step_index);
        self.select(item, step, media_changed, ports, errors);
        true
    }

    pub(super) fn media(
        &mut self,
        session_id: Option<SessionId>,
        action: MediaAction,
        ports: &mut Ports,
        errors: &mut Vec<EngineEvent>,
    ) {
        let Some(playback) = self.state.playback.as_mut() else {
            errors.push(error("media", "No media is cued."));
            return;
        };
        if let Some(id) = session_id.filter(|&id| id != playback.session_id) {
            errors.push(error("media", format!("Ignored a command for an earlier media session ({id}).")));
            return;
        }
        let session_id = playback.session_id;
        match action {
            MediaAction::Play if playback.paused => {
                if playback.phase == PlaybackPhase::Ended {
                    let start = playback.start_ms;
                    seek(playback, start, ports);
                }
                playback.paused = false;
                if playback.running() {
                    ports.media.send(MediaDirective::Play { session_id });
                }
            }
            MediaAction::Pause if !playback.paused => {
                let was_running = playback.running();
                playback.paused = true;
                if was_running {
                    ports.media.send(MediaDirective::Pause { session_id });
                }
            }
            MediaAction::Play | MediaAction::Pause => {}
            MediaAction::Stop => {
                if playback.running() {
                    ports.media.send(MediaDirective::Pause { session_id });
                }
                playback.paused = true;
                let start = playback.start_ms;
                seek(playback, start, ports);
            }
            MediaAction::Seek { position_ms } => {
                let end = match (playback.end_ms, playback.duration_ms) {
                    (Some(end), Some(duration)) => Some(end.min(duration)),
                    (end, duration) => end.or(duration),
                };
                let position = end
                    .map(|end| position_ms.min(end.saturating_sub(1)))
                    .unwrap_or(position_ms)
                    .max(playback.start_ms);
                seek(playback, position, ports);
            }
            MediaAction::SetMuted { muted } => {
                if playback.muted != muted {
                    playback.muted = muted;
                    ports.media.send(MediaDirective::Configure { playback: playback.clone() });
                }
            }
            MediaAction::SetVolume { volume } => {
                let volume = volume.min(100);
                if playback.volume != volume {
                    playback.volume = volume;
                    ports.media.send(MediaDirective::Configure { playback: playback.clone() });
                }
            }
        }
    }
}
