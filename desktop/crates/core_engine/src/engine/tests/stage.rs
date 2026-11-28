use super::*;
use protocol::{StageTimer, TimerAction};

#[test]
fn the_stage_timer_counts_down_pauses_and_goes_over() {
    let mut h = setup();
    h.load();
    let events = h.send(EngineCommand::StageTimer { action: TimerAction::Start { duration_ms: 60_000 } });
    assert!(events.iter().any(|e| matches!(e, EngineEvent::ProgramChanged { .. })));
    let timer = |h: &Harness| h.engine.snapshot().program_state.stage.timer.expect("a timer");
    assert_eq!(timer(&h), StageTimer { duration_ms: 60_000, remaining_ms: 60_000, running_since_ms: Some(1_000) });
    h.now_ms = 21_000;
    h.send(EngineCommand::StageTimer { action: TimerAction::Pause });
    assert_eq!(timer(&h).remaining_ms, 40_000);
    assert_eq!(timer(&h).running_since_ms, None);
    h.now_ms = 99_000;
    assert_eq!(timer(&h).remaining_at(h.now_ms), 40_000);
    h.send(EngineCommand::StageTimer { action: TimerAction::Resume });
    h.send(EngineCommand::StageTimer { action: TimerAction::Adjust { delta_ms: -50_000 } });
    assert_eq!(timer(&h).remaining_at(99_000), -10_000);
    h.send(EngineCommand::StageTimer { action: TimerAction::Clear });
    assert_eq!(h.engine.snapshot().program_state.stage.timer, None);
}

#[test]
fn stage_messages_are_trimmed_and_cleared() {
    let mut h = setup();
    h.send(EngineCommand::SetStageMessage { text: "  Wrap up  ".into() });
    assert_eq!(h.engine.snapshot().program_state.stage.message, "Wrap up");
    h.send(EngineCommand::SetStageMessage { text: String::new() });
    assert_eq!(h.engine.snapshot().program_state.stage.message, "");
}

#[test]
fn scripture_slides_carry_their_reference_and_second_translation() {
    let mut h = setup();
    h.items()[2].kind = SessionItemKind::Text { steps: vec!["For God so loved".into(), "For God sent not".into()], parallel: vec!["Sapagka't".into(), "Sapagka't hindi".into()], captions: vec!["John 3:16 · KJV".into(), "John 3:17 · KJV".into()] };
    h.load();
    h.send(EngineCommand::Cue { item_id: "30".into(), index: 1 });
    let p = &h.engine.snapshot().program_state;
    assert_eq!((p.active_text.as_str(), p.active_parallel.as_str(), p.active_caption.as_str()), ("For God sent not", "Sapagka't hindi", "John 3:17 · KJV"));
    h.send(EngineCommand::Cue { item_id: "10".into(), index: 0 });
    let p = &h.engine.snapshot().program_state;
    assert_eq!((p.active_parallel.as_str(), p.active_caption.as_str()), ("", ""));
}
