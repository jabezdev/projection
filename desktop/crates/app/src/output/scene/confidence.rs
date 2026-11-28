use protocol::{
    DisplayMode, LiveService, OutputConfig, PlaybackState, ProgramState, SessionItemKind,
};
use render_engine::{
    css_color, ConfidenceGroup, ConfidenceScene, StageClock, StageOverlay,
};

use super::*;

pub fn confidence_sample_scene(config: &OutputConfig) -> ConfidenceScene {
    let group = |key: &str, code: &str, color: &str, lines: &[&str]| ConfidenceGroup {
        key: key.into(),
        code: code.into(),
        title: "Your Love Never Fails".into(),
        color: css_color(color, [1.0; 4]),
        lines: lines.iter().map(|l| l.to_string()).collect(),
    };
    ConfidenceScene {
        groups: vec![
            group("v1", "V1", "#4f91e8", &["Nothing can separate\nEven if I ran away", "Your love never fails"]),
            group("c1", "C1", "#e76774", &["Your love never fails\nIt never gives up", "It never runs out on me"]),
            group("b1", "BR1", "#aa84df", &["You make all things work together for my good"]),
        ],
        active: Some((1, 0)),
        size: config.confidence_size as f32,
        video: None,
        video_cover: false,
        transition_ms: config.confidence_transition as f32,
        style: confidence_style(config),
        stage: StageOverlay::default(),
    }
}


pub fn confidence_groups(service: Option<&LiveService>) -> Vec<(String, usize, ConfidenceGroup)> {
    let mut groups = Vec::new();
    for item in service.map(|s| s.items.as_slice()).unwrap_or_default() {
        let title = item.title.clone().filter(|t| !t.is_empty());
        let (kind, steps) = match &item.kind {
            SessionItemKind::Text { steps, .. } => ("text", steps),
            SessionItemKind::Image { .. } => ("image", &Vec::new()),
            SessionItemKind::Video { .. } => ("video", &Vec::new()),
            SessionItemKind::Audio { .. } => ("audio", &Vec::new()),
            SessionItemKind::Presentation { .. } => ("presentation", &Vec::new()),
            SessionItemKind::Capture { .. } => ("capture", &Vec::new()),
            SessionItemKind::Section {} => continue,
        };
        if kind != "text" {
            let label = title.clone().unwrap_or_else(|| kind.to_string());
            groups.push((
                item.id.clone(),
                0,
                ConfidenceGroup {
                    key: item.id.clone(),
                    code: if kind == "presentation" { "Slides".into() } else { kind.into() },
                    title: label.clone(),
                    color: css_color(NON_SONG_COLOR, [1.0; 4]),
                    lines: vec![label],
                },
            ));
            continue;
        }
        let Some(song) = &item.song else {
            groups.push((
                item.id.clone(),
                0,
                ConfidenceGroup { key: item.id.clone(), code: "Text".into(), title: title.clone().unwrap_or_default(), color: css_color(NON_SONG_COLOR, [1.0; 4]), lines: steps.clone() },
            ));
            continue;
        };
        let mut first_step = 0;
        for (occurrence, section) in song.sections.iter().enumerate() {
            let lines = section.slides();
            groups.push((
                item.id.clone(),
                first_step,
                ConfidenceGroup {
                    key: format!("{}-{occurrence}", item.id),
                    code: section.code(),
                    title: title.clone().unwrap_or_default(),
                    color: css_color(section.color(), [1.0; 4]),
                    lines: lines.clone(),
                },
            ));
            first_step += lines.len();
        }
    }
    groups
}

pub fn confidence_scene(config: &OutputConfig, program: &ProgramState, service: Option<&LiveService>, playback: Option<&PlaybackState>) -> ConfidenceScene {
    let groups = confidence_groups(service);
    let step = program.active_step_index.unwrap_or(0) as usize;
    let active = program.active_item_id.as_ref().and_then(|id| {
        groups
            .iter()
            .position(|(item, start, g)| item == id && step >= *start && step < start + g.lines.len())
            .map(|index| (index, step - groups[index].1))
    });
    let item = active_item(program, service);
    let live = program.display == DisplayMode::Live;
    let shows_video = live && matches!(item.map(|i| &i.kind), Some(SessionItemKind::Video { options, .. }) if options.confidence);
    let capture = capture_on(item, true).filter(|_| live);
    ConfidenceScene {
        groups: groups.into_iter().map(|(_, _, g)| g).collect(),
        active,
        size: config.confidence_size as f32,
        video_cover: capture.as_ref().is_some_and(|(_, cover)| *cover),
        video: match capture {
            Some((key, _)) => Some(key),
            None if shows_video => foreground_video(item, playback),
            None => None,
        },
        transition_ms: config.confidence_transition as f32,
        style: confidence_style(config),
        stage: stage_overlay(program),
    }
}

pub fn stage_overlay(program: &ProgramState) -> StageOverlay {
    let stage = &program.stage;
    StageOverlay {
        message: stage.message.clone(),
        timer: stage.timer.map(|t| StageClock { remaining_ms: t.remaining_ms, ends_at_ms: t.running_since_ms.map(|since| since + t.remaining_ms) }),
    }
}
