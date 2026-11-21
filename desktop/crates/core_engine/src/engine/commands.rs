use protocol::{
    Background, DisplayMode, EngineCommand, EngineEvent,
    MediaAction, MediaDirective, PlaybackKind, PlaybackPhase, ProjectModel, SessionId,
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

            EngineCommand::LoadProject { project_id } => match ports.store.load_project(project_id) {
                Ok(project) => {
                    self.state.project = Some(project);
                    self.song_background = None;
                    self.select(None, None, true, ports, errors);
                    return true;
                }
                Err(e) => errors.push(error(
                    "project",
                    format!("Failed to load project {project_id}: {e:#}"),
                )),
            },
            EngineCommand::SaveProject { mut project } => {
                if let Err(e) = ports.store.save_project(&mut project) {
                    errors.push(error(
                        "project",
                        format!("Failed to save project {}: {e:#}", project.id),
                    ));
                    return false;
                }
                if self.state.project.as_ref().map(|p| p.id) != Some(project.id) {
                    return false;
                }
                let active_id = self.state.program_state.active_item_id;
                let find = |p: &ProjectModel| p.session.items.iter().find(|item| Some(item.id) == active_id).cloned();
                let old_item = self.state.project.as_ref().and_then(find);
                let new_item = find(&project);
                let media_changed = old_item.as_ref().map(|i| (&i.background, &i.kind))
                    != new_item.as_ref().map(|i| (&i.background, &i.kind));
                self.state.project = Some(project);
                let program = &self.state.program_state;
                let (item, step) = (program.active_item_id, program.active_step_index);
                self.select(item, step, media_changed, ports, errors);
                return true;
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
                let Some(project) = &self.state.project else {
                    errors.push(error(
                        "navigation",
                        "No project loaded; navigation has no effect.",
                    ));
                    return false;
                };
                let (item, step) = navigation_target(project, &self.state.program_state, &nav);
                self.select(item, step, false, ports, errors);
            }
        }
        false
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
