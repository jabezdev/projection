//! Windows capture. Windows.Graphics.Capture (the default) captures a
//! monitor or a single window, even a covered one, and reports when the
//! window closes. Desktop Duplication is the fallback for drivers where it
//! misbehaves: whole monitors only, without the mouse pointer.
//!
//! Sources are identified by the monitor's device name (`\\.\DISPLAY1`,
//! shared by both methods) or a window's program and title.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use protocol::{CaptureKind, CaptureSource};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl};
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use super::{Backend, CaptureStream, FrameSlot};

const INTERVAL: Duration = Duration::from_millis(33);

pub fn wgc_supported() -> bool {
    GraphicsCaptureApi::is_supported().unwrap_or(false)
}

fn monitors() -> anyhow::Result<Vec<CaptureSource>> {
    let mut sources = Vec::new();
    for (index, monitor) in Monitor::enumerate().map_err(|e| anyhow::anyhow!("{e}"))?.into_iter().enumerate() {
        let device = monitor.device_name().map_err(|e| anyhow::anyhow!("{e}"))?;
        let label = monitor.device_string().unwrap_or_default();
        let size = match (monitor.width(), monitor.height()) {
            (Ok(w), Ok(h)) => format!(" ({w}×{h})"),
            _ => String::new(),
        };
        sources.push(CaptureSource { kind: CaptureKind::Screen, id: format!("monitor:{device}"), name: format!("Display {} {label}{size}", index + 1).replace("  ", " ") });
    }
    Ok(sources)
}

fn window_key(window: &Window) -> Option<(String, String)> {
    let title = window.title().ok().filter(|t| !t.trim().is_empty())?;
    Some((window.process_name().unwrap_or_default(), title))
}

/// Windows to offer: titled, not PALTO's own (capturing an output would loop).
fn windows() -> anyhow::Result<Vec<(Window, String, String)>> {
    let own = std::process::id();
    Ok(Window::enumerate()
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .into_iter()
        .filter(|w| w.is_valid() && w.process_id().ok() != Some(own))
        .filter_map(|w| window_key(&w).map(|(program, title)| (w, program, title)))
        .collect())
}

fn find_monitor(device: &str) -> anyhow::Result<Monitor> {
    Monitor::enumerate()
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .into_iter()
        .find(|m| m.device_name().is_ok_and(|d| d == device))
        .ok_or_else(|| anyhow::anyhow!("The display {device} is not connected"))
}

/// The window with this program and title; else the program's only window
/// (titles often change, such as a document's name).
fn find_window(key: &str, name: &str) -> anyhow::Result<Window> {
    let (program, title) = key.split_once('|').unwrap_or(("", key));
    let candidates = windows()?;
    if let Some((window, ..)) = candidates.iter().find(|(_, p, t)| p == program && t == title) {
        return Ok(*window);
    }
    let same_program: Vec<_> = candidates.iter().filter(|(_, p, _)| p == program).collect();
    match same_program.as_slice() {
        [(window, ..)] => Ok(*window),
        _ => anyhow::bail!("The window \"{name}\" is not open"),
    }
}

pub struct Wgc;

struct Handler {
    slot: FrameSlot,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = FrameSlot;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self { slot: context.flags })
    }

    fn on_frame_arrived(&mut self, frame: &mut Frame, _control: InternalCaptureControl) -> Result<(), Self::Error> {
        let mut buffer = frame.buffer()?;
        let (width, height, pitch) = (buffer.width(), buffer.height(), buffer.row_pitch() as usize);
        let bytes = buffer.as_raw_buffer();
        if let Some(rows) = bytes.get(..pitch * height as usize) {
            self.slot.publish(width, height, pitch, rows.to_vec());
        }
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.slot.lose("The window was closed".into());
        Ok(())
    }
}

type Control = CaptureControl<Handler, Box<dyn std::error::Error + Send + Sync>>;

struct WgcRunning(Option<Control>);

impl CaptureStream for WgcRunning {}

impl Drop for WgcRunning {
    fn drop(&mut self) {
        if let Some(control) = self.0.take() {
            let _ = control.stop();
        }
    }
}

fn settings<T: windows_capture::settings::TryIntoCaptureItemWithType>(item: T, slot: FrameSlot) -> Settings<FrameSlot, T> {
    // Older Windows 10 builds refuse these options rather than ignore them.
    let supported = |check: Result<bool, windows_capture::graphics_capture_api::Error>| check.unwrap_or(false);
    Settings::new(
        item,
        if supported(GraphicsCaptureApi::is_cursor_settings_supported()) { CursorCaptureSettings::WithCursor } else { CursorCaptureSettings::Default },
        if supported(GraphicsCaptureApi::is_border_settings_supported()) { DrawBorderSettings::WithoutBorder } else { DrawBorderSettings::Default },
        SecondaryWindowSettings::Default,
        if supported(GraphicsCaptureApi::is_minimum_update_interval_supported()) { MinimumUpdateIntervalSettings::Custom(INTERVAL) } else { MinimumUpdateIntervalSettings::Default },
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        slot,
    )
}

impl Backend for Wgc {
    fn name(&self) -> &'static str {
        "wgc"
    }

    fn label(&self) -> &'static str {
        "Windows.Graphics.Capture"
    }

    fn sources(&self) -> anyhow::Result<Vec<CaptureSource>> {
        let mut sources = monitors()?;
        for (_, program, title) in windows()? {
            let label = if program.is_empty() { title.clone() } else { format!("{title} — {}", program.trim_end_matches(".exe")) };
            sources.push(CaptureSource { kind: CaptureKind::Window, id: format!("window:{program}|{title}"), name: label });
        }
        Ok(sources)
    }

    fn start(&self, source: &CaptureSource, slot: FrameSlot) -> anyhow::Result<Box<dyn CaptureStream>> {
        let control = if let Some(device) = source.id.strip_prefix("monitor:") {
            Handler::start_free_threaded(settings(find_monitor(device)?, slot))
        } else if let Some(key) = source.id.strip_prefix("window:") {
            Handler::start_free_threaded(settings(find_window(key, &source.name)?, slot))
        } else {
            anyhow::bail!("This source belongs to another capture method")
        };
        Ok(Box::new(WgcRunning(Some(control.map_err(|e| anyhow::anyhow!("{e}"))?))))
    }
}

pub struct Dxgi;

struct DxgiRunning(Arc<AtomicBool>);

impl CaptureStream for DxgiRunning {}

impl Drop for DxgiRunning {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Backend for Dxgi {
    fn name(&self) -> &'static str {
        "dxgi"
    }

    fn label(&self) -> &'static str {
        "Desktop Duplication (whole screens)"
    }

    fn sources(&self) -> anyhow::Result<Vec<CaptureSource>> {
        monitors()
    }

    fn start(&self, source: &CaptureSource, slot: FrameSlot) -> anyhow::Result<Box<dyn CaptureStream>> {
        let Some(device) = source.id.strip_prefix("monitor:").map(str::to_string) else {
            anyhow::bail!("Desktop Duplication captures whole screens only; choose Windows.Graphics.Capture for windows");
        };
        // Fail now, not on the capture thread, when the display is missing.
        let mut duplication = duplication::Duplication::open(&device)?;
        let running = Arc::new(AtomicBool::new(true));
        let keep = running.clone();
        std::thread::Builder::new().name("capture-dxgi".into()).spawn(move || {
            while keep.load(Ordering::Acquire) {
                let started = Instant::now();
                match duplication.next_frame(&slot) {
                    Ok(()) => {}
                    Err(duplication::Problem::AccessLost) => {
                        // A mode change, the secure desktop or a driver reset: reopen.
                        std::thread::sleep(Duration::from_millis(200));
                        match duplication::Duplication::open(&device) {
                            Ok(reopened) => duplication = reopened,
                            Err(e) if keep.load(Ordering::Acquire) => return slot.lose(format!("{e:#}")),
                            Err(_) => return,
                        }
                    }
                    Err(duplication::Problem::Failed(e)) => return slot.lose(e),
                }
                std::thread::sleep(INTERVAL.saturating_sub(started.elapsed()));
            }
        })?;
        Ok(Box::new(DxgiRunning(running)))
    }
}

mod duplication {
    use windows::core::Interface;
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT,
        DXGI_OUTDUPL_FRAME_INFO,
    };

    use super::super::FrameSlot;

    pub enum Problem {
        AccessLost,
        Failed(String),
    }

    pub struct Duplication {
        context: ID3D11DeviceContext,
        device: ID3D11Device,
        output: IDXGIOutputDuplication,
        staging: Option<(ID3D11Texture2D, (u32, u32))>,
    }

    // The thread that opens a duplication is the only one that uses it.
    unsafe impl Send for Duplication {}

    impl Duplication {
        /// Duplicates the output named `device` (`\\.\DISPLAY1`).
        pub fn open(device: &str) -> anyhow::Result<Self> {
            unsafe {
                let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
                let mut adapter_index = 0;
                while let Ok(adapter) = factory.EnumAdapters1(adapter_index) {
                    adapter_index += 1;
                    let mut output_index = 0;
                    while let Ok(output) = adapter.EnumOutputs(output_index) {
                        output_index += 1;
                        let desc = output.GetDesc()?;
                        let name = String::from_utf16_lossy(&desc.DeviceName);
                        if name.trim_end_matches('\0') != device {
                            continue;
                        }
                        let (mut d3d, mut context) = (None, None);
                        D3D11CreateDevice(&adapter, D3D_DRIVER_TYPE_UNKNOWN, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut d3d), None, Some(&mut context))?;
                        let (device, context) = (d3d.ok_or_else(|| anyhow::anyhow!("No Direct3D device"))?, context.ok_or_else(|| anyhow::anyhow!("No Direct3D context"))?);
                        let output: IDXGIOutput1 = output.cast()?;
                        let duplication = output.DuplicateOutput(&device).map_err(|e| anyhow::anyhow!("Desktop Duplication is not available for {name}: {e}"))?;
                        return Ok(Self { context, device, output: duplication, staging: None });
                    }
                }
                anyhow::bail!("The display {device} is not connected")
            }
        }

        /// Copies the next changed desktop image into `slot` (nothing when
        /// the screen did not change within a tenth of a second).
        pub fn next_frame(&mut self, slot: &FrameSlot) -> Result<(), Problem> {
            unsafe {
                let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
                let mut resource: Option<IDXGIResource> = None;
                match self.output.AcquireNextFrame(100, &mut info, &mut resource) {
                    Ok(()) => {}
                    Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(()),
                    Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST => return Err(Problem::AccessLost),
                    Err(e) => return Err(Problem::Failed(format!("Desktop Duplication failed: {e}"))),
                }
                let copied = self.copy(resource, info.LastPresentTime != 0, slot);
                let _ = self.output.ReleaseFrame();
                copied.map_err(|e| Problem::Failed(format!("Desktop Duplication failed: {e}")))
            }
        }

        unsafe fn copy(&mut self, resource: Option<IDXGIResource>, changed: bool, slot: &FrameSlot) -> windows::core::Result<()> {
            // Pointer-only updates carry no new image.
            let Some(resource) = resource.filter(|_| changed) else { return Ok(()) };
            let texture: ID3D11Texture2D = resource.cast()?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);
            let size = (desc.Width, desc.Height);
            if self.staging.as_ref().is_none_or(|(_, s)| *s != size) {
                let staging_desc = D3D11_TEXTURE2D_DESC {
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    MiscFlags: 0,
                    ..desc
                };
                let mut staging = None;
                self.device.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
                self.staging = staging.map(|t| (t, size));
            }
            let Some((staging, _)) = &self.staging else { return Ok(()) };
            self.context.CopyResource(staging, &texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let pitch = mapped.RowPitch as usize;
            let bytes = std::slice::from_raw_parts(mapped.pData as *const u8, pitch * size.1 as usize).to_vec();
            self.context.Unmap(staging, 0);
            slot.publish(size.0, size.1, pitch, bytes);
            Ok(())
        }
    }
}
