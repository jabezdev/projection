//! Scene behavior rendered offscreen: transitions, readiness and overrides.
//! Skipped when no GPU adapter is available.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use render_engine::{
    Capture, ConfidenceGroup, ConfidenceScene, Direction, Display, LyricStyle, MainScene, MediaLayer, MediaSource, OutputRole,
    PixelFrame, Scene, Stroke, Transition, TransitionKind, VideoKey,
};

const SIZE: (u32, u32) = (640, 360);

fn capture(role: OutputRole) -> Option<Capture> {
    match Capture::new(role) {
        Ok(capture) => Some(capture),
        Err(e) => {
            eprintln!("No GPU; skipping scene tests: {e:#}");
            None
        }
    }
}

fn pixel(frame: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE.0 + x) * 4) as usize;
    [frame[i], frame[i + 1], frame[i + 2], frame[i + 3]]
}

/// Brightest gray level inside a rectangle.
fn brightest(frame: &[u8], x0: u32, y0: u32, x1: u32, y1: u32) -> u8 {
    let mut best = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let [r, g, b, _] = pixel(frame, x, y);
            best = best.max(r.max(g).max(b));
        }
    }
    best
}

fn solid_png(name: &str, rgb: [u8; 3]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("scene-fixtures");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let pixels: Vec<u8> = (0..64 * 36).flat_map(|_| rgb).collect();
    image::save_buffer(&path, &pixels, 64, 36, image::ColorType::Rgb8).unwrap();
    path
}

fn main_scene(text: &str) -> MainScene {
    MainScene { text: text.into(), transition_ms: 200.0, style: LyricStyle { size: 8.0, ..Default::default() }, ..Default::default() }
}

/// White lyrics in the bottom half, measured on red so a blue box does not count.
fn text_brightness(frame: &[u8]) -> u8 {
    let mut best = 0;
    for y in 180..SIZE.1 {
        for x in 0..SIZE.0 {
            best = best.max(pixel(frame, x, y)[0]);
        }
    }
    best
}

#[test]
fn lyrics_crossfade_on_one_clock_and_the_box_stays() {
    let Some(mut capture) = capture(OutputRole::Projection) else { return };
    let boxed = LyricStyle { size: 8.0, box_color: [0.0, 0.0, 1.0, 1.0], ..Default::default() };
    let t0 = Instant::now();
    capture.set_scene(Scene::Main(MainScene { style: boxed.clone(), ..main_scene("FIRST") }), t0);
    let start = capture.render(SIZE, t0);
    assert!(text_brightness(&start) < 255, "new text starts transparent");
    let settled = capture.render(SIZE, t0 + Duration::from_millis(250));
    assert!(text_brightness(&settled) >= 250, "fully faded in");
    assert!(!capture.busy(t0 + Duration::from_millis(250)));

    let t1 = t0 + Duration::from_millis(300);
    capture.set_scene(Scene::Main(MainScene { style: boxed, ..main_scene("") }), t1);
    let half = capture.render(SIZE, t1 + Duration::from_millis(100));
    let level = text_brightness(&half);
    assert!((100..200).contains(&level), "outgoing text half faded: {level}");
    // The box (blue) persists while the outgoing text fades.
    assert!(half.chunks_exact(4).any(|p| p[2] == 255 && p[0] == 0), "box still drawn");
    let gone = capture.render(SIZE, t1 + Duration::from_millis(250));
    assert_eq!(brightest(&gone, 0, 180, SIZE.0, SIZE.1), 0, "box and text removed after the fade");
}

#[test]
fn rapid_cues_keep_only_the_newest_and_one_outgoing_text() {
    let Some(mut capture) = capture(OutputRole::Projection) else { return };
    let t0 = Instant::now();
    for (i, text) in ["A", "B", "C", "D"].iter().enumerate() {
        capture.set_scene(Scene::Main(main_scene(text)), t0 + Duration::from_millis(20 * i as u64));
    }
    // 60 ms in: D fading in from 0, C fading out from its partial opacity.
    let frame = capture.render(SIZE, t0 + Duration::from_millis(120));
    assert!(text_brightness(&frame) > 0);
    let done = capture.render(SIZE, t0 + Duration::from_millis(400));
    assert!(text_brightness(&done) >= 250);
    assert!(!capture.busy(t0 + Duration::from_millis(400)));
}

#[test]
fn black_hides_the_background_and_logo_shows_a_fallback() {
    let Some(mut capture) = capture(OutputRole::Projection) else { return };
    let red = solid_png("red.png", [255, 0, 0]);
    let background = Some(MediaLayer { source: MediaSource::Image(red.to_str().unwrap().into()), cover: true });
    let t0 = Instant::now();
    capture.set_scene(Scene::Main(MainScene { background: background.clone(), ..main_scene("") }), t0);
    let (frame, t1) = capture.settle(SIZE, t0, Duration::from_secs(5));
    assert_eq!(pixel(&frame, 10, 10), [255, 0, 0, 255], "image background shown once loaded");

    capture.set_scene(Scene::Main(MainScene { background: background.clone(), display: Display::Black, ..main_scene("") }), t1);
    let frame = capture.render(SIZE, t1);
    assert_eq!(pixel(&frame, 10, 10), [0, 0, 0, 255], "Black covers at once");

    capture.set_scene(Scene::Main(MainScene { background, display: Display::Logo, ..main_scene("hidden") }), t1);
    let frame = capture.render(SIZE, t1 + Duration::from_millis(300));
    assert_eq!(pixel(&frame, 10, 10), [0, 0, 0, 255]);
    assert!(brightest(&frame, 200, 120, 440, 240) > 200, "MAGI fallback in the middle");
}

#[test]
fn a_failed_image_keeps_the_previous_background() {
    let Some(mut capture) = capture(OutputRole::Projection) else { return };
    let green = solid_png("green.png", [0, 255, 0]);
    let t0 = Instant::now();
    let layer = |path: &str| Some(MediaLayer { source: MediaSource::Image(path.into()), cover: true });
    capture.set_scene(Scene::Main(MainScene { background: layer(green.to_str().unwrap()), ..main_scene("") }), t0);
    let (_, t1) = capture.settle(SIZE, t0, Duration::from_secs(5));
    capture.set_scene(Scene::Main(MainScene { background: layer("/nonexistent/missing.png"), ..main_scene("") }), t1);
    std::thread::sleep(Duration::from_millis(200));
    let frame = capture.render(SIZE, t1 + Duration::from_secs(2));
    assert_eq!(pixel(&frame, 10, 10), [0, 255, 0, 255], "no black frame for an asset that never loads");
}

fn upload(capture: &Capture, key: &VideoKey, rgba: [u8; 4]) {
    struct Frame(Vec<u8>, u64);
    impl PixelFrame for Frame {
        fn id(&self) -> u64 {
            self.1
        }
        fn size(&self) -> (u32, u32) {
            (16, 9)
        }
        fn stride(&self) -> usize {
            64
        }
        fn pixels(&self) -> &[u8] {
            &self.0
        }
    }
    let frame = Frame((0..16 * 9).flat_map(|_| rgba).collect(), 1);
    let texture = capture.media().video(key);
    assert!(texture.upload(&capture.gpu().queue, &frame));
    capture.gpu().queue.submit(None);
}

#[test]
fn a_video_layer_appears_only_with_its_first_frame() {
    let Some(mut capture) = capture(OutputRole::Projection) else { return };
    let blue = solid_png("blue.png", [0, 0, 255]);
    let t0 = Instant::now();
    capture.set_scene(
        Scene::Main(MainScene {
            background: Some(MediaLayer { source: MediaSource::Image(blue.to_str().unwrap().into()), cover: true }),
            ..main_scene("")
        }),
        t0,
    );
    let (_, t1) = capture.settle(SIZE, t0, Duration::from_secs(5));
    let key = VideoKey::Session(42);
    capture.set_scene(Scene::Main(MainScene { background: Some(MediaLayer { source: MediaSource::Video(key.clone()), cover: true }), ..main_scene("") }), t1);
    let waiting = capture.render(SIZE, t1 + Duration::from_millis(500));
    assert_eq!(pixel(&waiting, 10, 10), [0, 0, 255, 255], "previous background holds while the video prepares");
    upload(&capture, &key, [255, 255, 0, 255]);
    let t2 = t1 + Duration::from_millis(600);
    capture.render(SIZE, t2);
    let mid = capture.render(SIZE, t2 + Duration::from_millis(100));
    let [r, _, b, _] = pixel(&mid, 10, 10);
    assert!(r > 50 && b > 50, "crossfading: {:?}", pixel(&mid, 10, 10));
    let done = capture.render(SIZE, t2 + Duration::from_millis(300));
    assert_eq!(pixel(&done, 10, 10), [255, 255, 0, 255]);
}

#[test]
fn confidence_video_replaces_the_teleprompter_once_ready() {
    let Some(mut capture) = capture(OutputRole::Confidence) else { return };
    let group = ConfidenceGroup { key: "1".into(), code: "V1".into(), title: "Song".into(), color: [0.3, 0.5, 0.9, 1.0], lines: vec!["line".into()] };
    let teleprompter = ConfidenceScene { groups: vec![group], active: Some((0, 0)), size: 4.0, video: None, transition_ms: 200.0, style: Default::default() };
    let t0 = Instant::now();
    capture.set_scene(Scene::Confidence(teleprompter.clone()), t0);
    let lyrics = capture.render(SIZE, t0 + Duration::from_secs(1));
    let key = VideoKey::Session(9);
    capture.set_scene(Scene::Confidence(ConfidenceScene { video: Some(key.clone()), ..teleprompter.clone() }), t0 + Duration::from_secs(1));
    let before = capture.render(SIZE, t0 + Duration::from_millis(1100));
    assert_eq!(pixel(&before, 320, 200), pixel(&lyrics, 320, 200), "teleprompter stays until the video has a frame");
    upload(&capture, &key, [255, 0, 255, 255]);
    let t1 = t0 + Duration::from_millis(1200);
    capture.render(SIZE, t1);
    let shown = capture.render(SIZE, t1 + Duration::from_millis(300));
    assert_eq!(pixel(&shown, 320, 200), [255, 0, 255, 255], "16:9 video fills a 16:9 output");
    capture.set_scene(Scene::Confidence(teleprompter), t1 + Duration::from_millis(300));
    let back = capture.render(SIZE, t1 + Duration::from_millis(700));
    assert_eq!(pixel(&back, 320, 200), pixel(&lyrics, 320, 200), "teleprompter returns when the video leaves");
}

#[test]
fn a_logo_image_is_letterboxed_within_the_margins() {
    let Some(mut capture) = capture(OutputRole::Projection) else { return };
    let white = solid_png("white-square.png", [255, 255, 255]);
    let t0 = Instant::now();
    let scene = MainScene { display: Display::Logo, logo: Some(MediaSource::Image(white.to_str().unwrap().into())), ..main_scene("") };
    capture.set_scene(Scene::Main(scene), t0);
    let (frame, _) = capture.settle(SIZE, t0, Duration::from_secs(5));
    // 64x36 image contained in the area inside 5cq / 10*9/16cq margins.
    let (cq, area_y) = (6.4, 10.0 * 9.0 / 16.0 * 6.4);
    assert_eq!(pixel(&frame, SIZE.0 / 2, SIZE.1 / 2), [255, 255, 255, 255]);
    assert_eq!(pixel(&frame, (5.0 * cq) as u32 - 2, SIZE.1 / 2), [0, 0, 0, 255], "left margin stays black");
    assert_eq!(pixel(&frame, SIZE.0 / 2, area_y as u32 - 2), [0, 0, 0, 255], "top margin stays black");
}

#[test]
#[ignore]
fn long_service_layout_time() {
    let Some(mut capture) = capture(OutputRole::Confidence) else { return };
    let groups: Vec<ConfidenceGroup> = (0..300)
        .map(|i| ConfidenceGroup {
            key: i.to_string(),
            code: format!("V{}", i % 5 + 1),
            title: "A long song title for the tape".into(),
            color: [0.3, 0.5, 0.9, 1.0],
            lines: (0..6).map(|l| format!("Line {l} of part {i} with enough words to wrap sometimes\nand a second line")).collect(),
        })
        .collect();
    let t0 = Instant::now();
    let scene = ConfidenceScene { groups, active: Some((150, 2)), size: 4.0, video: None, transition_ms: 200.0, style: Default::default() };
    capture.set_scene(Scene::Confidence(scene.clone()), t0);
    let started = Instant::now();
    capture.render((1920, 1080), t0);
    let first = started.elapsed();
    // Edit one line: the rebuild runs on a worker and reuses shaped text.
    let mut edited = scene;
    edited.groups[10].lines[0] = "An edited line".into();
    capture.set_scene(Scene::Confidence(edited), t0);
    let started = Instant::now();
    capture.render((1920, 1080), t0 + Duration::from_millis(16));
    let during = started.elapsed();
    let started = Instant::now();
    while capture.busy(t0 + Duration::from_secs(10)) {
        std::thread::sleep(Duration::from_millis(1));
        capture.render((1920, 1080), t0 + Duration::from_secs(10));
    }
    println!("1800 rows: first layout {first:?}; frame during a rebuild {during:?}; edited rebuild done in {:?}", started.elapsed());
}

/// Columns (x) in the lower half where text is drawn.
fn lit_columns(frame: &[u8]) -> Vec<u32> {
    (0..SIZE.0).filter(|&x| (180..SIZE.1).any(|y| pixel(frame, x, y)[0] > 128)).collect()
}

#[test]
fn a_cut_shows_the_new_text_at_once() {
    let Some(mut capture) = capture(OutputRole::Projection) else { return };
    let cut = Transition { kind: TransitionKind::Cut, ..Default::default() };
    let t0 = Instant::now();
    capture.set_scene(Scene::Main(MainScene { transition: cut, ..main_scene("FIRST") }), t0);
    assert!(text_brightness(&capture.render(SIZE, t0)) >= 250, "no fade in");
    assert!(!capture.busy(t0 + Duration::from_millis(1)));
}

#[test]
fn a_slide_moves_the_new_text_in_from_behind_the_movement() {
    let Some(mut capture) = capture(OutputRole::Projection) else { return };
    let slide = Transition { kind: TransitionKind::Slide, direction: Direction::Left, ..Default::default() };
    let t0 = Instant::now();
    capture.set_scene(Scene::Main(MainScene { transition: slide, ..main_scene("I") }), t0);
    let settled = lit_columns(&capture.render(SIZE, t0 + Duration::from_millis(300)));
    let middle = capture.render(SIZE, t0 + Duration::from_millis(100));
    let moving = lit_columns(&middle);
    assert!(!settled.is_empty() && !moving.is_empty());
    assert!(moving[0] > settled[0] + 50, "halfway, the text is still right of its place: {} vs {}", moving[0], settled[0]);
    assert!(text_brightness(&middle) >= 250, "sliding text is opaque");
}

#[test]
fn an_outline_surrounds_the_glyphs() {
    let Some(mut capture) = capture(OutputRole::Projection) else { return };
    let red = solid_png("outline-red.png", [255, 0, 0]);
    let background = Some(MediaLayer { source: MediaSource::Image(red.to_str().unwrap().into()), cover: true });
    let outlined = LyricStyle { size: 8.0, outline: Some(Stroke { color: [0.0, 0.0, 1.0, 1.0], size: 16.0 }), ..Default::default() };
    let t0 = Instant::now();
    capture.set_scene(Scene::Main(MainScene { background, style: outlined, ..main_scene("O") }), t0);
    let (frame, _) = capture.settle(SIZE, t0, Duration::from_secs(5));
    let blue = frame.chunks_exact(4).filter(|p| p[2] > 200 && p[0] < 60).count();
    assert!(blue > 100, "an outline in the outline color: {blue} pixels");
    assert!(text_brightness(&frame) >= 250, "the glyph fill is drawn over it");
}
