use std::time::Duration;


use crate::Gpu;


/// A non-sRGB, opaque, vsynced configuration for `surface`, and the format
/// its views are drawn in (blending on sRGB-encoded values, as in browsers).
pub(crate) fn surface_config(
    gpu: &Gpu,
    surface: &wgpu::Surface<'_>,
    label: &str,
    width: u32,
    height: u32,
) -> anyhow::Result<(wgpu::SurfaceConfiguration, wgpu::TextureFormat)> {
    let caps = surface.get_capabilities(&gpu.adapter);
    anyhow::ensure!(!caps.formats.is_empty(), "{label} surface has no supported formats");
    let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
    let view_format = format.remove_srgb_suffix();
    let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
        wgpu::CompositeAlphaMode::Opaque
    } else {
        caps.alpha_modes[0]
    };
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: width.max(1),
        height: height.max(1),
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode,
        view_formats: if view_format != format { vec![view_format] } else { vec![] },
        desired_maximum_frame_latency: 2,
        color_space: wgpu::SurfaceColorSpace::Auto,
    };
    log::info!("{label} surface: {format:?} {width}x{height}");
    Ok((config, view_format))
}

/// Applies `config`, retrying briefly: wgpu refuses a configure while another
/// thread submits work (the frame pump, another output, the previews or a
/// monitor), reporting it only as an error. Returns whether it applied.
pub(crate) fn configure_surface(gpu: &Gpu, surface: &wgpu::Surface<'_>, config: &wgpu::SurfaceConfiguration, label: &str) -> bool {
    const ATTEMPTS: u32 = 5;
    for attempt in 1..=ATTEMPTS {
        let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        surface.configure(&gpu.device, config);
        match pollster::block_on(scope.pop()) {
            None => return true,
            Some(error) if attempt == ATTEMPTS => {
                log::warn!("{label} surface configure failed; retrying next frame: {error}");
            }
            Some(_) => std::thread::sleep(Duration::from_millis(2)),
        }
    }
    false
}

/// A 60 Hz refresh, in milliseconds.
pub(super) const REFRESH_MS: f64 = 1000.0 / 60.0;

/// Refreshes missed by an interval between back-to-back frames: none up to
/// 1.5 refreshes (vsync jitter), else the whole refreshes skipped. Assumes a
/// 60 Hz or faster display; on a slower one every frame counts, as it should.
pub(super) fn dropped_refreshes(interval_ms: f64) -> u64 {
    if interval_ms <= REFRESH_MS * 1.5 {
        return 0;
    }
    ((interval_ms / REFRESH_MS).round() as u64).saturating_sub(1).max(1)
}

#[cfg(test)]
mod dropped_tests {
    use super::dropped_refreshes;

    #[test]
    fn counts_whole_refreshes_missed_and_ignores_jitter() {
        assert_eq!(dropped_refreshes(0.0), 0);
        assert_eq!(dropped_refreshes(16.7), 0);
        assert_eq!(dropped_refreshes(22.0), 0);
        assert_eq!(dropped_refreshes(33.3), 1);
        assert_eq!(dropped_refreshes(50.0), 2);
        // A 75 Hz display's single miss still counts.
        assert_eq!(dropped_refreshes(26.7), 1);
    }
}
