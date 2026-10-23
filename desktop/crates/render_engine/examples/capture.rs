//! Renders a named scene offscreen to a PNG, for comparison with the parity
//! references (`capture <scene> <width> <height> <out.png>`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use render_engine::{
    css_color, Capture, ConfidenceGroup, ConfidenceScene, Display, LyricStyle, MainScene, MediaLayer, MediaSource, OutputRole, PixelFrame, Scene,
    Shadow, ShaderSpec, Stroke, TextAlign, VideoKey, Vertical,
};

/// A deterministic gradient: red across, green down, blue fixed.
fn gradient(width: u32, height: u32) -> Vec<u8> {
    (0..height).flat_map(|y| (0..width).flat_map(move |x| [(x * 255 / width) as u8, (y * 255 / height) as u8, 160, 255])).collect()
}

/// The gradient as a PNG in the temporary folder, for image backgrounds and logos.
fn gradient_png() -> String {
    let path: PathBuf = std::env::temp_dir().join("magi-parity-gradient.png");
    image::save_buffer(&path, &gradient(320, 180), 320, 180, image::ColorType::Rgba8).expect("write gradient");
    path.to_string_lossy().into_owned()
}

/// One still video frame, standing in for a decoder.
struct StillFrame(Vec<u8>);

impl PixelFrame for StillFrame {
    fn id(&self) -> u64 {
        1
    }
    fn size(&self) -> (u32, u32) {
        (640, 360)
    }
    fn stride(&self) -> usize {
        640 * 4
    }
    fn pixels(&self) -> &[u8] {
        &self.0
    }
}

const VIDEO: VideoKey = VideoKey::Session(1);

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
                ..LyricStyle::default()
            },
            ..Default::default()
        }),
        "proj-effects" => Scene::Main(MainScene {
            text,
            background: Some(MediaLayer { source: MediaSource::Image(gradient_png()), cover: true }),
            transition_ms: 250.0,
            style: LyricStyle {
                shadow: Some(Shadow { color: [0.0, 0.0, 0.0, 0.8], x: 0.0, y: 6.0, blur: 10.0 }),
                outline: Some(Stroke { color: css_color("#14213d", [1.0; 4]), size: 3.0 }),
                glow: Some(Stroke { color: [1.0, 0.9, 0.5, 0.6], size: 14.0 }),
                ..LyricStyle::default()
            },
            ..Default::default()
        }),
        shader if shader.starts_with("proj-shader-") => Scene::Main(MainScene {
            text,
            background: Some(MediaLayer { source: MediaSource::Video(VideoKey::Shader(ShaderSpec::encode(&shader["proj-shader-".len()..], "#14213d", "#47c6bd", 1.0, false))), cover: true }),
            transition_ms: 250.0,
            ..Default::default()
        }),
        "proj-black" => Scene::Main(MainScene { text, display: Display::Black, background: Some(MediaLayer { source: MediaSource::Image(gradient_png()), cover: true }), transition_ms: 250.0, ..Default::default() }),
        "proj-logo" => Scene::Main(MainScene { text, display: Display::Logo, logo: Some(MediaSource::Image(gradient_png())), transition_ms: 250.0, ..Default::default() }),
        "proj-logo-fallback" => Scene::Main(MainScene { text, display: Display::Logo, transition_ms: 250.0, ..Default::default() }),
        "proj-video" => Scene::Main(MainScene { text, background: Some(MediaLayer { source: MediaSource::Video(VIDEO), cover: true }), transition_ms: 250.0, ..Default::default() }),
        "conf-video" => Scene::Confidence(ConfidenceScene { size: 4.0, video: Some(VIDEO), transition_ms: 250.0, ..Default::default() }),
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
            style: Default::default(),
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
    // Video scenes show one still frame, uploaded as the frame pump would.
    if name.ends_with("-video") {
        capture.media().video(&VIDEO).upload(&capture.gpu().queue, &StillFrame(gradient(640, 360)));
        capture.gpu().queue.submit(None);
    }
    let start = Instant::now();
    capture.set_scene(scene, start);
    capture.render_shaders();
    let (pixels, _) = capture.settle(size, start, Duration::from_secs(5));
    image::save_buffer(out, &pixels, size.0, size.1, image::ColorType::Rgba8)?;
    Ok(())
}
