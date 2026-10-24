//! Hardware video decoding through FFmpeg's hwaccels. Decoded frames are
//! copied back to system memory as NV12 and take the same path as software
//! frames from there, so a hardware decoder only takes the decoding work off
//! the CPU. Software decoding is always the fallback: when a device cannot
//! be opened, the codec or profile is not supported, or decoding fails
//! midway, the session continues in software.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::{Mutex, OnceLock, RwLock};

use ffmpeg_next as ffmpeg;
use ffmpeg::ffi;

/// Which decoder new sessions use.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DecoderChoice {
    /// Software only.
    #[default]
    Software,
    /// The first working accelerator in [`candidates`] order.
    Auto,
    /// One accelerator by FFmpeg device name (e.g. `d3d11va`, `vaapi`).
    Device(String),
}

impl DecoderChoice {
    /// `"software"`, `"auto"` or a device name; anything unknown is software.
    pub fn parse(value: &str) -> Self {
        match value {
            "auto" => DecoderChoice::Auto,
            "" | "software" => DecoderChoice::Software,
            name if candidates().iter().any(|(n, _)| *n == name) => DecoderChoice::Device(name.to_string()),
            _ => DecoderChoice::Software,
        }
    }
}

/// Accelerators worth trying on this platform, in the order Auto tries them,
/// with a label for the settings page. Intel Quick Sync and AMD AMF are
/// reached through D3D11VA (Windows) and VA-API (Linux): FFmpeg uses them
/// as separate decoders rather than hwaccels.
pub fn candidates() -> &'static [(&'static str, &'static str)] {
    if cfg!(windows) {
        &[
            ("d3d11va", "Direct3D 11 (D3D11VA)"),
            ("cuda", "NVIDIA (NVDEC)"),
            ("dxva2", "DirectX Video Acceleration 2"),
            ("d3d12va", "Direct3D 12"),
            ("vulkan", "Vulkan Video"),
        ]
    } else if cfg!(target_os = "macos") {
        &[("videotoolbox", "VideoToolbox")]
    } else {
        &[
            ("vaapi", "VA-API"),
            ("cuda", "NVIDIA (NVDEC)"),
            ("vdpau", "VDPAU"),
            ("vulkan", "Vulkan Video"),
        ]
    }
}

static CHOICE: RwLock<DecoderChoice> = RwLock::new(DecoderChoice::Software);

/// Applies to sessions opened from now on; open ones keep their decoder.
pub fn set_choice(choice: DecoderChoice) {
    *CHOICE.write().expect("decoder choice poisoned") = choice;
}

pub fn choice() -> DecoderChoice {
    CHOICE.read().expect("decoder choice poisoned").clone()
}

/// An FFmpeg hardware device context (`AVBufferRef`).
struct Device(*mut ffi::AVBufferRef);

// FFmpeg device contexts are reference counted and safe to share between
// decoders on different threads.
unsafe impl Send for Device {}
unsafe impl Sync for Device {}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { ffi::av_buffer_unref(&mut self.0) };
    }
}

fn device_type(name: &str) -> Option<ffi::AVHWDeviceType> {
    let name = CString::new(name).ok()?;
    let kind = unsafe { ffi::av_hwdevice_find_type_by_name(name.as_ptr()) };
    (kind != ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE).then_some(kind)
}

fn create_device(kind: ffi::AVHWDeviceType) -> Result<Device, ffmpeg::Error> {
    let mut device = std::ptr::null_mut();
    match unsafe { ffi::av_hwdevice_ctx_create(&mut device, kind, std::ptr::null(), std::ptr::null_mut(), 0) } {
        0 => Ok(Device(device)),
        code => Err(ffmpeg::Error::from(code)),
    }
}

/// One device per accelerator, opened on first use and kept for the app's
/// lifetime (opening one can take a few hundred milliseconds). `None` once
/// opening it failed.
fn shared_device(name: &str) -> Option<&'static Device> {
    static DEVICES: OnceLock<Mutex<HashMap<String, Option<&'static Device>>>> = OnceLock::new();
    let mut devices = DEVICES.get_or_init(Mutex::default).lock().expect("hw devices poisoned");
    *devices.entry(name.to_string()).or_insert_with(|| {
        let kind = device_type(name)?;
        match create_device(kind) {
            Ok(device) => {
                log::info!("Hardware decoding: opened {name}");
                Some(&*Box::leak(Box::new(device)))
            }
            Err(e) => {
                log::warn!("Hardware decoding: {name} is unavailable: {e}");
                None
            }
        }
    })
}

/// Accelerators that open on this computer, probed once (each device is
/// opened and closed again). FFmpeg builds without one leave it out.
pub fn available() -> Vec<(&'static str, &'static str)> {
    static AVAILABLE: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
    AVAILABLE
        .get_or_init(|| {
            let _ = ffmpeg::init();
            candidates()
                .iter()
                .copied()
                .filter(|(name, _)| device_type(name).is_some_and(|kind| create_device(kind).is_ok()))
                .collect()
        })
        .clone()
}

/// The hardware pixel format `codec` produces with `kind`, if it supports it.
fn hw_format(codec: *const ffi::AVCodec, kind: ffi::AVHWDeviceType) -> Option<ffi::AVPixelFormat> {
    (0..)
        .map(|i| unsafe { ffi::avcodec_get_hw_config(codec, i) })
        .take_while(|config| !config.is_null())
        .map(|config| unsafe { *config })
        .find(|config| config.methods & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32 != 0 && config.device_type == kind)
        .map(|config| config.pix_fmt)
}

/// A decoder set up for hardware decoding.
#[derive(Debug, Clone, Copy)]
pub struct HwDecoder {
    pub name: &'static str,
    /// Frames in this format live on the GPU and must be transferred.
    pub format: ffi::AVPixelFormat,
}

/// Picks hardware frames when the decoder offers them, otherwise the
/// software format, so an unsupported stream decodes in software instead of
/// failing.
unsafe extern "C" fn get_format(context: *mut ffi::AVCodecContext, formats: *const ffi::AVPixelFormat) -> ffi::AVPixelFormat {
    let wanted = (*context).opaque as isize as i32;
    let mut software = ffi::AVPixelFormat::AV_PIX_FMT_NONE;
    let mut p = formats;
    while *p != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
        if *p as i32 == wanted {
            return *p;
        }
        let descriptor = ffi::av_pix_fmt_desc_get(*p);
        if software == ffi::AVPixelFormat::AV_PIX_FMT_NONE
            && !descriptor.is_null()
            && (*descriptor).flags & ffi::AV_PIX_FMT_FLAG_HWACCEL as u64 == 0
        {
            software = *p;
        }
        p = p.add(1);
    }
    log::warn!("Hardware decoding is not supported for this video; decoding in software");
    software
}

/// Sets `context` up for the chosen accelerator before it is opened.
/// Returns `None` (leaving it a software decoder) when nothing applies.
pub fn attach(context: &mut ffmpeg::codec::context::Context, codec: *const ffi::AVCodec) -> Option<HwDecoder> {
    let names: Vec<&'static str> = match choice() {
        DecoderChoice::Software => return None,
        DecoderChoice::Auto => candidates().iter().map(|(name, _)| *name).collect(),
        DecoderChoice::Device(name) => candidates().iter().map(|(n, _)| *n).filter(|n| *n == name).collect(),
    };
    for name in names {
        let Some(kind) = device_type(name) else { continue };
        let Some(format) = hw_format(codec, kind) else { continue };
        let Some(device) = shared_device(name) else { continue };
        unsafe {
            let raw = context.as_mut_ptr();
            (*raw).hw_device_ctx = ffi::av_buffer_ref(device.0);
            (*raw).get_format = Some(get_format);
            (*raw).opaque = format as i32 as isize as *mut std::ffi::c_void;
        }
        return Some(HwDecoder { name, format });
    }
    None
}

/// Copies a hardware frame into system memory (NV12, or P010 for 10-bit),
/// keeping its timestamps and color description.
pub fn download(frame: &ffmpeg::frame::Video) -> Result<ffmpeg::frame::Video, ffmpeg::Error> {
    let mut software = ffmpeg::frame::Video::empty();
    unsafe {
        match ffi::av_hwframe_transfer_data(software.as_mut_ptr(), frame.as_ptr(), 0) {
            0 => {}
            code => return Err(ffmpeg::Error::from(code)),
        }
        match ffi::av_frame_copy_props(software.as_mut_ptr(), frame.as_ptr()) {
            0 => {}
            code => return Err(ffmpeg::Error::from(code)),
        }
    }
    Ok(software)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_parse_with_software_as_the_fallback() {
        assert_eq!(DecoderChoice::parse("auto"), DecoderChoice::Auto);
        assert_eq!(DecoderChoice::parse("software"), DecoderChoice::Software);
        assert_eq!(DecoderChoice::parse("no-such-accelerator"), DecoderChoice::Software);
        let (first, _) = candidates()[0];
        assert_eq!(DecoderChoice::parse(first), DecoderChoice::Device(first.to_string()));
    }
}
