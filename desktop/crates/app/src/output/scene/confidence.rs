
use core_engine::song::Song;
use protocol::{
    DisplayMode, OutputConfig, PlaybackState, ProgramState, ProjectModel, SessionItemKind,
};
use render_engine::{
    css_color, ConfidenceGroup, ConfidenceScene,
};

use super::*;

/// A short song in the confidence style, for the output settings dialog.
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
    }
}

/// Teleprompter groups: one per song part occurrence, one per other item.
pub fn confidence_groups(project: Option<&ProjectModel>) -> Vec<(i64, usize, ConfidenceGroup)> {
    let mut groups = Vec::new();
    for item in project.map(|p| p.session.items.as_slice()).unwrap_or_default() {
        let title = item.title.clone().filter(|t| !t.is_empty());
        let (kind, steps) = match &item.kind {
            SessionItemKind::Text { steps } => ("text", steps),
            SessionItemKind::Image { .. } => ("image", &Vec::new()),
            SessionItemKind::Video { .. } => ("video", &Vec::new()),
            SessionItemKind::Audio { .. } => ("audio", &Vec::new()),
            SessionItemKind::Presentation { .. } => ("presentation", &Vec::new()),
            SessionItemKind::Capture { .. } => ("capture", &Vec::new()),
            // Headings group the operator's list; the stage does not need them.
            SessionItemKind::Section {} => continue,
        };
        if kind != "text" {
            let label = title.clone().unwrap_or_else(|| kind.to_string());
            groups.push((
                item.id,
                0,
                ConfidenceGroup {
                    key: item.id.to_string(),
                    code: if kind == "presentation" { "Slides".into() } else { kind.into() },
                    title: label.clone(),
                    color: css_color(NON_SONG_COLOR, [1.0; 4]),
                    lines: vec![label],
                },
            ));
            continue;
        }
        let song = Song::decode(item.song.as_deref().unwrap_or(&steps.join("\n\n")));
        for occurrence in song.occurrences() {
            groups.push((
                item.id,
                occurrence.first_step,
                ConfidenceGroup {
                    key: format!("{}-{}", item.id, occurrence.occurrence),
                    code: occurrence.part.code(),
                    title: title.clone().unwrap_or_default(),
                    color: css_color(occurrence.part.color(), [1.0; 4]),
                    lines: occurrence.slides.clone(),
                },
            ));
        }
    }
    groups
}

pub fn confidence_scene(config: &OutputConfig, program: &ProgramState, project: Option<&ProjectModel>, playback: Option<&PlaybackState>) -> ConfidenceScene {
    let groups = confidence_groups(project);
    let step = program.active_step_index.unwrap_or(0) as usize;
    let active = program.active_item_id.and_then(|id| {
        groups
            .iter()
            .position(|(item, start, g)| *item == id && step >= *start && step < start + g.lines.len())
            .map(|index| (index, step - groups[index].1))
    });
    let item = active_item(program, project);
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
    }
}
