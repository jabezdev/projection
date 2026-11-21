use std::sync::Arc;
use std::time::{Duration, Instant};

use protocol::{CaptureKind, CaptureSource};
use render_engine::{FramePixels, PixelFrame};

use super::*;

struct Fake;
struct Running;
impl CaptureStream for Running {}
impl Backend for Fake {
    fn name(&self) -> &'static str {
        "fake"
    }
    fn label(&self) -> &'static str {
        "Fake"
    }
    fn sources(&self) -> anyhow::Result<Vec<CaptureSource>> {
        Ok(vec![])
    }
    fn start(&self, source: &CaptureSource, slot: FrameSlot) -> anyhow::Result<Box<dyn CaptureStream>> {
        anyhow::ensure!(source.id != "missing", "not found");
        slot.publish(2, 2, 8, vec![255; 16]);
        Ok(Box::new(Running))
    }
}

fn source(id: &str) -> CaptureSource {
    CaptureSource { kind: CaptureKind::Screen, id: id.into(), name: id.into() }
}

#[test]
fn captures_follow_the_active_sources_and_failures_are_reported() {
    let captures = Captures::with_backends(vec![Arc::new(Fake)], "");
    captures.set_active(&[source("a"), source("missing")]);
    assert!(captures.frame("a").is_some());
    let status = captures.status();
    assert_eq!(status.iter().find(|s| s.id == "missing").unwrap().error.as_deref(), Some("not found"));
    let black = captures.frame("missing").expect("a source that cannot start shows black");
    assert!(matches!(black.pixels(), FramePixels::Bgra { data, .. } if data.iter().all(|&b| b == 0)));
    captures.set_active(&[]);
    assert!(captures.frame("a").is_none() && captures.status().is_empty(), "stopped when no longer on air");
}

#[test]
fn a_lost_source_turns_black_instead_of_freezing() {
    let slot = FrameSlot::default();
    slot.publish(4, 2, 16, vec![200; 32]);
    let first = slot.frame(Instant::now()).unwrap();
    slot.lose("window closed".into());
    let after = slot.frame(Instant::now()).unwrap();
    assert_ne!(after.id, first.id);
    assert_eq!((after.width, after.height), (4, 2), "black in the same size");
    assert!(after.data.iter().all(|&b| b == 0));
    assert_eq!(slot.lost().as_deref(), Some("window closed"));
}

#[test]
fn frames_are_capped_but_the_newest_always_arrives() {
    let slot = FrameSlot::default();
    let start = Instant::now();
    slot.publish(2, 2, 8, vec![1; 16]);
    let a = slot.frame(start).unwrap().id;
    slot.publish(2, 2, 8, vec![2; 16]);
    assert_eq!(slot.frame(start + Duration::from_millis(10)).unwrap().id, a, "too soon: the previous frame stays");
    assert_ne!(slot.frame(start + Duration::from_millis(40)).unwrap().id, a, "then the newest");
}
