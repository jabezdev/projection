use std::sync::Arc;

use anyhow::Context;

use crate::*;

/// Device and queue shared by every output and frame source.
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub(super) yuv: std::sync::OnceLock<yuv::YuvPipeline>,
}

impl Gpu {
    pub(super) fn open(instance: wgpu::Instance, adapter: wgpu::Adapter) -> anyhow::Result<Self> {
        let info = adapter.get_info();
        log::info!("Native output adapter: {} ({:?}, {:?})", info.name, info.backend, info.device_type);
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Native output device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            },
        ))?;
        device.on_uncaptured_error(Arc::new(|error| log::error!("GPU error: {error}")));
        Ok(Self { instance, adapter, device, queue, yuv: std::sync::OnceLock::new() })
    }

    /// A device without surfaces, for captures and tests.
    pub fn headless() -> anyhow::Result<Self> {
        let instance = new_instance();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .context("No GPU adapter available")?;
        Self::open(instance, adapter)
    }
}

/// Vulkan and DX12 need no instance display handle; each surface brings its
/// window's own (`SurfaceTarget::DisplayAndWindow`). `WGPU_BACKEND`
/// (`vulkan`, `dx12`) narrows the choice, for comparing them on one computer.
pub(super) fn new_instance() -> wgpu::Instance {
    let backends = match wgpu::Backends::from_env() {
        Some(chosen) => {
            log::info!("GPU backends limited by WGPU_BACKEND to {chosen:?}");
            chosen
        }
        None => wgpu::Backends::PRIMARY,
    };
    wgpu::Instance::new(wgpu::InstanceDescriptor { backends, ..wgpu::InstanceDescriptor::new_without_display_handle() })
}
