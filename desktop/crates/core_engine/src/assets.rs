use protocol::{Background, BackgroundKind};
use crate::engine::Assets;

/// Checks backgrounds on the local file system.
pub struct Files;
impl Assets for Files {
    fn check_background(&self, background: &Background) -> anyhow::Result<()> {
        let metadata = std::fs::metadata(&background.path)?;
        anyhow::ensure!(metadata.is_file(), "Background must be a file");
        if background.kind == BackgroundKind::Image { image::image_dimensions(&background.path)?; }
        Ok(())
    }
}
