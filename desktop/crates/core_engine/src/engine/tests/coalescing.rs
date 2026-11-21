use super::*;

#[test]
fn coalescing_keeps_the_latest_scrub_and_preserves_order() {
    let seek = |position_ms| EngineInput::Command(EngineCommand::Media { session_id: Some(7), action: MediaAction::Seek { position_ms } });
    let cue = EngineInput::Command(EngineCommand::Cue { item_id: 1, index: 0 });
    let merged = coalesce(vec![seek(1), seek(2), seek(3), cue, seek(4), seek(5)]);
    let positions: Vec<_> = merged
        .iter()
        .map(|input| match input {
            EngineInput::Command(EngineCommand::Media { action: MediaAction::Seek { position_ms }, .. }) => Some(*position_ms),
            _ => None,
        })
        .collect();
    assert_eq!(positions, vec![Some(3), None, Some(5)]);
}

#[test]
fn coalescing_never_merges_different_sessions_projects_or_phases() {
    let seek = |session_id| EngineInput::Command(EngineCommand::Media { session_id: Some(session_id), action: MediaAction::Seek { position_ms: 0 } });
    assert_eq!(coalesce(vec![seek(1), seek(2)]).len(), 2);
    let save = |id| EngineInput::Command(EngineCommand::SaveProject { project: ProjectModel::empty(id, "p") });
    assert_eq!(coalesce(vec![save(1), save(1), save(2)]).len(), 2);
    let report = |phase| EngineInput::Report(PlaybackReport { session_id: 1, generation: 0, phase, position_ms: 0, duration_ms: None, error: None });
    assert_eq!(coalesce(vec![report(PlaybackPhase::Playing), report(PlaybackPhase::Playing), report(PlaybackPhase::Ended)]).len(), 2);
}
