use protocol::{Background, BackgroundKind};
use crate::engine::Assets;

/// Checks backgrounds on the local file system.
pub struct Files;
impl Assets for Files {
    fn check_background(&self, background: &Background) -> anyhow::Result<()> {
        if background.kind == BackgroundKind::Shader {
            let preset = background.path.trim_start_matches(protocol::SHADER_PREFIX);
            anyhow::ensure!(protocol::SHADER_PRESETS.contains(&preset), "Unknown shader background {preset}");
            return Ok(());
        }
        let metadata = std::fs::metadata(&background.path)?;
        anyhow::ensure!(metadata.is_file(), "Background must be a file");
        if background.kind == BackgroundKind::Image { image::image_dimensions(&background.path)?; }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_backgrounds_must_name_a_built_in_preset() {
        let shader = |path: &str| Background { kind: BackgroundKind::Shader, path: path.into() };
        assert!(Files.check_background(&shader("shader:aurora")).is_ok());
        assert!(Files.check_background(&shader("shader:nonexistent")).is_err());
    }
}
