use protocol::Background;
use crate::engine::Output;

/// Validates engine output before it is published to the projection webview.
/// The webview exclusively owns rendering; a native swapchain on the same
/// window can overwrite WebKit's text and video after it has been composited.
pub struct Projector;
impl Output for Projector {
    fn show_text(&mut self, _text: &str) {}
    fn show_background(&mut self, background: &Background) -> anyhow::Result<()> {
        let metadata = std::fs::metadata(&background.path)?;
        anyhow::ensure!(metadata.is_file(), "Background must be a file");
        if background.kind == protocol::BackgroundKind::Image { image::image_dimensions(&background.path)?; }
        Ok(())
    }
    fn clear_background(&mut self) {}
    fn set_opacity(&mut self, layer: usize, _value: f32) -> anyhow::Result<()> {
        anyhow::ensure!(layer <= 1, "Unknown layer {layer}"); Ok(())
    }
    fn resize(&mut self, _width: u32, _height: u32) {}
}
