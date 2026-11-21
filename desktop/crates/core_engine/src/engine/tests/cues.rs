use super::*;

#[test]
fn navigation_passes_over_section_headings() {
    let mut h = setup();
    {
        let mut projects = h.store.projects.borrow_mut();
        let items = &mut projects[0].session.items;
        items.insert(0, section(5));
        items.insert(2, section(15));
        items.insert(4, section(25));
    }
    h.load();
    assert_eq!(selection(&h.engine), (Some(10), Some(0), "a"), "a service never opens on a heading");
    assert_eq!(h.engine.snapshot().program_state.next_item_id, Some(20));
    h.send(EngineCommand::Next);
    h.send(EngineCommand::Next);
    assert_eq!(selection(&h.engine).0, Some(20));
    h.send(EngineCommand::NextItem);
    assert_eq!(selection(&h.engine).0, Some(30));
    h.send(EngineCommand::PrevItem);
    assert_eq!(selection(&h.engine).0, Some(20));
    h.send(EngineCommand::Prev);
    assert_eq!(selection(&h.engine), (Some(10), Some(0), "a"), "back to the song, not a heading");
    h.send(EngineCommand::GoToItem { item_id: 25 });
    assert_eq!(selection(&h.engine).0, Some(30), "cueing a heading goes to its first item");
}

#[test]
fn cue_selects_item_and_slide_in_one_event_without_showing_first_slide() {
    let mut h = setup();
    h.load();
    h.send(EngineCommand::GoToItem { item_id: 30 });
    let events = h.send(EngineCommand::Cue { item_id: 10, index: 1 });
    assert_eq!(selection(&h.engine), (Some(10), Some(1), "b"));
    assert_eq!(h.engine.snapshot().program_state.active_text, "b");
    assert_eq!(events.len(), 1);
    match &events[0] {
        EngineEvent::ProgramChanged { program_state, .. } => {
            assert_eq!(program_state.active_text, "b");
            assert_eq!(program_state.active_step_index, Some(1));
        }
        event => panic!("Expected one final cue, got {event:?}"),
    }
}

#[test]
fn song_background_changes_on_selection_and_reuses_between_slides() {
    let mut h = setup();
    h.store.projects.borrow_mut()[0].session.items[0].background = Some("song.jpg".into());
    h.load();
    assert_eq!(*h.assets.backgrounds.borrow(), vec!["song.jpg"]);
    h.send(EngineCommand::GoToStep { index: 1 });
    assert_eq!(h.assets.backgrounds.borrow().len(), 1);
    h.send(EngineCommand::GoToItem { item_id: 30 });
    assert_eq!(
        h.engine
            .snapshot()
            .program_state
            .background
            .as_ref()
            .unwrap()
            .path,
        "song.jpg"
    );
    h.send(EngineCommand::GoToItem { item_id: 20 });
    assert_eq!(*h.assets.backgrounds.borrow(), vec!["song.jpg", "bg.png"]);
}

#[test]
fn editing_active_song_background_applies_without_reselecting() {
    let mut h = setup();
    h.load();
    let mut project = h.engine.snapshot().project.clone().unwrap();
    project.session.items[0].background = Some("edited.jpg".into());
    h.send(EngineCommand::SaveProject { project });
    assert_eq!(*h.assets.backgrounds.borrow(), vec!["edited.jpg"]);
    assert_eq!(selection(&h.engine), (Some(10), Some(0), "a"));
}

#[test]
fn display_override_keeps_selection_and_lyrics_for_return_to_live() {
    let mut h = setup();
    h.load();
    for display in [
        protocol::DisplayMode::Black,
        protocol::DisplayMode::Clear,
        protocol::DisplayMode::Logo,
        protocol::DisplayMode::Live,
    ] {
        let events = h.send(EngineCommand::SetDisplay { display });
        assert_eq!(h.engine.snapshot().program_state.display, display);
        assert_eq!(selection(&h.engine), (Some(10), Some(0), "a"));
        assert!(matches!(
            events.as_slice(),
            [EngineEvent::ProgramChanged { .. }]
        ));
    }
}

#[test]
fn load_project_emits_both_domains_with_one_version() {
    let mut h = setup();
    let events = h.send(EngineCommand::LoadProject { project_id: 1 });

    assert!(matches!(
        events.as_slice(),
        [
            EngineEvent::ProjectChanged { version: 1, .. },
            EngineEvent::ProgramChanged { version: 1, .. },
        ]
    ));
    assert_eq!(selection(&h.engine), (Some(10), Some(0), "a"));
    assert_eq!(h.engine.snapshot().program_state.next_item_id, Some(20));
    assert_eq!(h.engine.snapshot().program_state.active_text, "a");
}

#[test]
fn next_walks_steps_then_items_and_applies_media() {
    let mut h = setup();
    h.load();

    h.send(EngineCommand::Next);
    assert_eq!(selection(&h.engine), (Some(10), Some(1), "b"));

    h.send(EngineCommand::Next);
    assert_eq!(selection(&h.engine), (Some(20), None, ""));
    assert_eq!(*h.assets.backgrounds.borrow(), ["bg.png"]);
    assert_eq!(
        h.engine
            .snapshot()
            .program_state
            .background
            .as_ref()
            .map(|b| b.path.as_str()),
        Some("bg.png")
    );

    h.send(EngineCommand::Next);
    assert_eq!(selection(&h.engine), (Some(30), Some(0), "c"));

    let events = h.send(EngineCommand::Next);
    assert!(events.is_empty());
}

#[test]
fn prev_at_first_step_goes_to_previous_item() {
    let mut h = setup();
    h.load();
    h.send(EngineCommand::GoToItem { item_id: 30 });

    h.send(EngineCommand::Prev);
    assert_eq!(selection(&h.engine), (Some(20), None, ""));
}

#[test]
fn navigation_without_project_is_an_error_without_version_bump() {
    let mut h = setup();
    let events = h.send(EngineCommand::Next);

    assert!(matches!(
        events.as_slice(),
        [EngineEvent::Error { source, .. }] if source == "navigation"
    ));
    assert_eq!(h.engine.snapshot().version, 0);
}

#[test]
fn unchanged_state_emits_nothing() {
    let mut h = setup();
    let black = || EngineCommand::SetDisplay { display: DisplayMode::Black };
    assert_eq!(h.send(black()).len(), 1);
    assert!(h.send(black()).is_empty());
    assert_eq!(h.engine.snapshot().version, 1);
}

#[test]
fn failed_background_keeps_previous_background() {
    let mut h = setup();
    h.assets.broken.push("missing.png".into());
    h.send(EngineCommand::SetBackground {
            path: "ok.png".into(),
        });

    let events = h.send(EngineCommand::SetBackground {
            path: "missing.png".into(),
        });
    assert!(matches!(events.as_slice(), [EngineEvent::Error { .. }]));
    assert_eq!(
        h.engine
            .snapshot()
            .program_state
            .background
            .as_ref()
            .map(|b| b.path.as_str()),
        Some("ok.png")
    );
}

#[test]
fn background_kind_is_inferred_from_extension() {
    assert_eq!(background_kind("clip.MP4"), BackgroundKind::Video);
    assert_eq!(background_kind("still.png"), BackgroundKind::Image);
    assert_eq!(background_kind("shader:aurora"), BackgroundKind::Shader);
}

#[test]
fn saving_another_project_leaves_loaded_project_alone() {
    let mut h = setup();
    h.store
        .projects
        .borrow_mut()
        .push(ProjectModel::empty(2, "Other"));
    h.load();

    let events = h.send(EngineCommand::SaveProject {
            project: ProjectModel::empty(2, "Other renamed"),
        });
    assert!(events.is_empty());
    assert_eq!(h.engine.snapshot().project.as_ref().map(|p| p.id), Some(1));
}

#[test]
fn saving_loaded_project_reresolves_selection() {
    let mut h = setup();
    h.load();

    let mut edited = h.engine.snapshot().project.clone().unwrap();
    edited.session.items[0] = text_item(10, &["a (edited)", "b"]);
    let events = h.send(EngineCommand::SaveProject { project: edited });

    assert_eq!(events.len(), 2);
    assert_eq!(selection(&h.engine), (Some(10), Some(0), "a (edited)"));
    assert_eq!(h.engine.snapshot().program_state.active_text, "a (edited)");
}
#[test]
fn presentation_steps_cue_images_and_follow_navigation() {
    let mut h = setup();
    h.store.projects.borrow_mut()[0].session.items[1].kind = SessionItemKind::Presentation { path: "first.png".into(), slides: vec!["first.png".into(), "second.png".into(), "third.png".into()] };
    h.load();
    h.send(EngineCommand::GoToItem { item_id: 20 });
    assert_eq!(selection(&h.engine), (Some(20), Some(0), ""));
    assert_eq!(h.assets.backgrounds.borrow().last().unwrap(), "first.png");
    h.send(EngineCommand::Next);
    assert_eq!(selection(&h.engine), (Some(20), Some(1), ""));
    assert_eq!(h.assets.backgrounds.borrow().last().unwrap(), "second.png");
    h.send(EngineCommand::GoToStep { index: 2 });
    assert_eq!(h.assets.backgrounds.borrow().last().unwrap(), "third.png");
    h.send(EngineCommand::Next);
    assert_eq!(selection(&h.engine), (Some(30), Some(0), "c"));
}

#[test]
fn editing_active_presentation_refreshes_the_selected_image() {
    let mut h = setup();
    h.store.projects.borrow_mut()[0].session.items[1].kind = SessionItemKind::Presentation { path: "first.png".into(), slides: vec!["first.png".into(), "second.png".into()] };
    h.load();
    h.send(EngineCommand::GoToItem { item_id: 20 });
    h.send(EngineCommand::GoToStep { index: 1 });
    let mut project = h.engine.snapshot().project.clone().unwrap();
    project.session.items[1].kind = SessionItemKind::Presentation { path: "first.png".into(), slides: vec!["first.png".into(), "edited.png".into()] };
    h.send(EngineCommand::SaveProject { project });
    assert_eq!(selection(&h.engine), (Some(20), Some(1), ""));
    assert_eq!(h.assets.backgrounds.borrow().last().unwrap(), "edited.png");
}
