use std::sync::Arc;
use std::time::Instant;

use crate::confidence::ConfidenceRenderer;
use crate::main_scene::MainRenderer;
use crate::media::MediaTextures;
use crate::paint::DrawList;
use crate::scene::{Activity, Scene};
use crate::text::Fonts;

use super::*;

pub(crate) enum Renderer {
    Main(MainRenderer),
    Confidence(ConfidenceRenderer),
}

impl Renderer {
    pub fn for_role(role: OutputRole) -> Self {
        match role {
            OutputRole::Projection => Renderer::Main(MainRenderer::default()),
            OutputRole::Confidence => Renderer::Confidence(ConfidenceRenderer::default()),
        }
    }

    pub fn set_scene(&mut self, scene: Scene, media: &MediaTextures, now: Instant) {
        match (self, scene) {
            (Renderer::Main(r), Scene::Main(s)) => r.set_scene(s, media, now),
            (Renderer::Confidence(r), Scene::Confidence(s)) => r.set_scene(s, media, now),
            _ => log::warn!("Scene does not match the output's role"),
        }
    }

    pub fn activity(&self, now: Instant) -> Activity {
        match self {
            Renderer::Main(r) => r.activity(now),
            Renderer::Confidence(r) => r.activity(now),
        }
    }

    pub fn frame(&mut self, fonts: &Arc<Fonts>, size: (u32, u32), now: Instant) -> DrawList {
        match self {
            Renderer::Main(r) => r.frame(fonts, size, now),
            Renderer::Confidence(r) => r.frame(fonts, size, now),
        }
    }
}
