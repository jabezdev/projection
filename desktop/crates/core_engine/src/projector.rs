use std::sync::Arc;

use anyhow::Context;
use log::error;
use protocol::{Background, BackgroundKind};

use crate::engine::Output;
use crate::render::GraphicsEngine;
use crate::scene::{TextNode, VideoNode};
use crate::video::VideoPlayer;

/// The projection output: a window surface plus its scene (background, then text).
pub struct Projector<'window> {
    gfx: GraphicsEngine<'window>,
    background: VideoNode,
    text: TextNode,
}

impl<'window> Projector<'window> {
    pub async fn new<W>(window: Arc<W>, width: u32, height: u32) -> anyhow::Result<Self>
    where
        W: raw_window_handle::HasWindowHandle
            + raw_window_handle::HasDisplayHandle
            + Send
            + Sync
            + 'static,
    {
        let mut gfx = GraphicsEngine::new(window, width, height).await?;
        let background = VideoNode::new(&gfx.device, &gfx.transform_bind_group_layout);
        let text = TextNode::new(&gfx.device, &mut gfx.text, width, height);
        Ok(Self {
            gfx,
            background,
            text,
        })
    }

    /// Whether frames change on their own (video playing) and need continuous rendering.
    pub fn is_animating(&self) -> bool {
        self.background.player.is_some()
    }

    pub fn render(&mut self) {
        self.background.update_from_player(
            &self.gfx.device,
            &self.gfx.queue,
            &self.gfx.texture_bind_group_layout,
        );

        match self.gfx.render(&mut [&mut self.background, &mut self.text]) {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let (width, height) = self.gfx.size();
                self.gfx.resize(width, height);
            }
            Err(wgpu::SurfaceError::OutOfMemory) => error!("Surface out of memory!"),
            Err(e) => error!("Render error: {e:?}"),
        }
    }
}

impl Output for Projector<'_> {
    fn show_text(&mut self, text: &str) {
        self.text.set_text(&mut self.gfx.text, text);
    }

    fn show_background(&mut self, background: &Background) -> anyhow::Result<()> {
        match background.kind {
            BackgroundKind::Image => {
                let bytes = std::fs::read(&background.path).context("read image")?;
                self.background.load_image(
                    &self.gfx.device,
                    &self.gfx.queue,
                    &self.gfx.texture_bind_group_layout,
                    &bytes,
                )
            }
            BackgroundKind::Video => {
                self.background.set_player(VideoPlayer::new(&background.path)?);
                Ok(())
            }
        }
    }

    fn set_opacity(&mut self, layer: usize, value: f32) -> anyhow::Result<()> {
        match layer {
            0 => self.background.opacity = value,
            1 => self.text.opacity = value,
            _ => anyhow::bail!("Unknown layer {layer} (0 = background, 1 = text)"),
        }
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.gfx.resize(width, height);
        self.text.resize(&mut self.gfx.text, width, height);
    }
}
