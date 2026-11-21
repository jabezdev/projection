use crate::Gpu;

use super::*;

#[test]
fn lays_out_at_output_size_up_to_1080p_width() {
    assert_eq!(layout_size((1280, 720)), (1280, 720));
    assert_eq!(layout_size((3840, 2160)), (1920, 1080));
    assert_eq!(layout_size((0, 0)), (1, 1));
}

#[test]
fn scales_a_full_size_frame_into_a_small_surface() {
    let gpu = Gpu::headless().expect("headless GPU");
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut scaler = Scaler::new(&gpu.device, FORMAT);
    let source = scaler.source(&gpu.device, (1920, 1080));
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: 480, height: 300, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &source,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
    }
    scaler.draw(&gpu, &mut encoder, &view, crate::fit_aspect((480, 300), Some((1920, 1080))));
    gpu.queue.submit(Some(encoder.finish()));
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    assert!(pollster::block_on(scope.pop()).is_none(), "the scale pass is valid");
    let _ = scaler.source(&gpu.device, (1280, 720));
    assert_eq!(scaler.source.as_ref().map(|(t, ..)| (t.width(), t.height())), Some((1280, 720)));
}

#[test]
fn uses_more_taps_for_larger_reductions() {
    assert_eq!(taps(1.0), 1);
    assert_eq!(taps(2.0), 1);
    assert_eq!(taps(4.0), 2);
    assert_eq!(taps(6.0), 3);
    assert_eq!(taps(20.0), 4);
}
