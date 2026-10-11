//! Renders a named scene offscreen to a PNG, for comparison with the parity
//! references (`capture <scene> <width> <height> <out.png>`).

use std::time::{Duration, Instant};

use render_engine::{css_color, Capture, ConfidenceGroup, ConfidenceScene, LyricStyle, MainScene, OutputRole, Scene, TextAlign, Vertical};

fn group(key: &str, code: &str, title: &str, color: &str, lines: &[&str]) -> ConfidenceGroup {
    ConfidenceGroup {
        key: key.into(),
        code: code.into(),
        title: title.into(),
        color: css_color(color, [1.0; 4]),
        lines: lines.iter().map(|l| l.to_string()).collect(),
    }
}

fn scene(name: &str) -> Scene {
    let text = "Amazing grace how sweet the sound\nThat saved a wretch like me".to_string();
    match name {
        "proj-default" => Scene::Main(MainScene { text, transition_ms: 250.0, ..Default::default() }),
        "proj-styled" => Scene::Main(MainScene {
            text,
            transition_ms: 250.0,
            style: LyricStyle {
                font: "Liberation Serif".into(),
                size: 5.0,
                weight: 400,
                italic: true,
                caps: true,
                color: css_color("#ffe08a", [1.0; 4]),
                align: TextAlign::Left,
                vertical: Vertical::Center,
                margin_x: 8.0,
                margin_y: 6.0,
                line_height: 1.5,
                letter_spacing: 8.0,
                border: 6.0,
                border_color: css_color("#47c6bd", [1.0; 4]),
                box_color: [0x20 as f32 / 255.0, 0x30 as f32 / 255.0, 0x40 as f32 / 255.0, 0.6],
            },
            ..Default::default()
        }),
        "conf-default" => Scene::Confidence(ConfidenceScene {
            groups: vec![
                group("1-0", "V1", "Amazing Grace", "#4f91e8", &["Amazing grace how sweet the sound\nThat saved a wretch like me", "I once was lost but now am found\nWas blind but now I see"]),
                group("1-1", "C1", "Amazing Grace", "#e76774", &["My chains are gone\nI have been set free", "My God my Savior has ransomed me"]),
                group("1-2", "BR1", "Amazing Grace", "#aa84df", &["The Lord has promised good to me"]),
                group("1-3", "C1", "Amazing Grace", "#e76774", &["My chains are gone\nI have been set free", "My God my Savior has ransomed me"]),
                group("2", "video", "Announcements", "#7d9baf", &["Announcements"]),
                group("3", "Slides", "presentation", "#7d9baf", &["presentation"]),
            ],
            active: Some((1, 1)),
            size: 4.0,
            video: None,
            transition_ms: 250.0,
        }),
        "conf-waiting" => Scene::Confidence(ConfidenceScene { size: 4.0, transition_ms: 250.0, ..Default::default() }),
        other => panic!("unknown scene {other}"),
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, name, width, height, out] = args.as_slice() else {
        anyhow::bail!("usage: capture <scene> <width> <height> <out.png>");
    };
    let size = (width.parse()?, height.parse()?);
    let scene = scene(name);
    let mut capture = Capture::new(OutputRole::for_scene(&scene))?;
    let start = Instant::now();
    capture.set_scene(scene, start);
    let (pixels, _) = capture.settle(size, start, Duration::from_secs(5));
    image::save_buffer(out, &pixels, size.0, size.1, image::ColorType::Rgba8)?;
    Ok(())
}
