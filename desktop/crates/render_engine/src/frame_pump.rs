use std::sync::Arc;

use crate::*;

pub(super) struct FramePump {
    pub(super) running: Arc<std::sync::atomic::AtomicBool>,
    pub(super) thread: Option<std::thread::JoinHandle<()>>,
}

pub(super) const PUMP_INTERVAL: std::time::Duration = std::time::Duration::from_millis(4);

impl FramePump {
    pub(super) fn start(gpu: Arc<Gpu>, media: Arc<MediaTextures>, frames: Arc<dyn VideoFrames>) -> anyhow::Result<Self> {
        use std::sync::atomic::{AtomicBool, Ordering};
        let running = Arc::new(AtomicBool::new(true));
        let thread = {
            let running = running.clone();
            std::thread::Builder::new().name("frame-pump".into()).spawn(move || {
                let mut shaders = shader_bg::ShaderBackgrounds::new(&gpu.device);
                while running.load(Ordering::Acquire) {
                    let mut uploaded = false;
                    let videos = media.videos();
                    let now = std::time::Instant::now();
                    for (key, texture) in &videos {
                        match key {
                            VideoKey::Shader(spec) => {
                                shaders.tick(&gpu, spec, texture, now);
                            }
                            _ => {
                                if let Some(next) = frames.frame(key).filter(|f| f.id() != texture.last_id()) {
                                    let started = std::time::Instant::now();
                                    uploaded |= texture.upload(&gpu, next.as_ref());
                                    profile::UPLOAD.record(started.elapsed());
                                }
                            }
                        }
                    }
                    shaders.retain(&videos.iter().filter_map(|(key, _)| match key { VideoKey::Shader(spec) => Some(spec), _ => None }).collect::<Vec<_>>());
                    let _ = gpu.device.poll(wgpu::PollType::Poll);
                    if uploaded {
                        // Writes are applied on the next submit; without one, an
                        // idle output would never see the frame.
                        gpu.queue.submit(None);
                    }
                    std::thread::sleep(PUMP_INTERVAL);
                }
            })?
        };
        Ok(Self { running, thread: Some(thread) })
    }

    pub(super) fn stop(mut self) {
        self.running.store(false, std::sync::atomic::Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
