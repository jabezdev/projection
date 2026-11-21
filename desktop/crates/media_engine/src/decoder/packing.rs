use std::sync::Arc;

use ffmpeg_next as ffmpeg;

use crate::frames::{FrameFormat, Plane, YuvMatrix};

use super::*;

pub(super) fn has_alpha(format: ffmpeg::format::Pixel) -> bool {
    let descriptor = unsafe { ffmpeg::ffi::av_pix_fmt_desc_get(format.into()) };
    !descriptor.is_null() && unsafe { (*descriptor).flags } & ffmpeg::ffi::AV_PIX_FMT_FLAG_ALPHA as u64 != 0
}

/// The matrix a frame declares, else the usual one for its size.
pub(super) fn yuv_matrix(frame: &ffmpeg::frame::Video) -> YuvMatrix {
    use ffmpeg::color::Space;
    match frame.color_space() {
        Space::BT709 => YuvMatrix::Bt709,
        Space::BT470BG | Space::SMPTE170M | Space::FCC | Space::SMPTE240M => YuvMatrix::Bt601,
        Space::BT2020NCL | Space::BT2020CL => YuvMatrix::Bt2020,
        _ if frame.height() >= 720 => YuvMatrix::Bt709,
        _ => YuvMatrix::Bt601,
    }
}

/// Copies the first `count` planes of `frame` into one shared buffer.
pub(super) fn copy_planes(frame: &ffmpeg::frame::Video, count: usize) -> ([Plane; 3], Arc<[u8]>) {
    let mut planes = [Plane::default(); 3];
    let mut total = 0;
    for (i, plane) in planes.iter_mut().enumerate().take(count) {
        *plane = Plane { offset: total, stride: frame.stride(i) };
        total += frame.data(i).len();
    }
    let mut data = Arc::<[u8]>::new_uninit_slice(total);
    let buffer = Arc::get_mut(&mut data).expect("new buffer is unique");
    for (i, plane) in planes.iter().enumerate().take(count) {
        let source = frame.data(i);
        // SAFETY: the planes were laid out back to back within `total` bytes.
        unsafe { std::ptr::copy_nonoverlapping(source.as_ptr(), buffer.as_mut_ptr().add(plane.offset) as *mut u8, source.len()) };
    }
    // SAFETY: every byte was written above.
    (planes, unsafe { data.assume_init() })
}

/// Packs `source` for upload: 8-bit 4:2:0 YUV as decoded when it fits
/// [`MAX_YUV_FRAME`], sources with transparency as RGBA, anything else
/// converted (and if needed scaled down) to 4:2:0 YUV.
pub(super) fn pack(video: &mut VideoStream, source: &ffmpeg::frame::Video) -> anyhow::Result<Packed> {
    use ffmpeg::format::Pixel;
    let (width, height) = (source.width(), source.height());
    let fits = width <= MAX_YUV_FRAME.0 && height <= MAX_YUV_FRAME.1;
    let matrix = yuv_matrix(source);
    let declared_full = source.color_range() == ffmpeg::color::Range::JPEG;
    let direct = match source.format() {
        Pixel::YUV420P if fits => Some((FrameFormat::I420, declared_full)),
        Pixel::YUVJ420P if fits => Some((FrameFormat::I420, true)),
        Pixel::NV12 if fits => Some((FrameFormat::Nv12, declared_full)),
        _ => None,
    };
    if let Some((format, full_range)) = direct {
        let (planes, data) = copy_planes(source, format.planes());
        return Ok(Packed { width, height, format, matrix, full_range, planes, data });
    }
    let alpha = has_alpha(source.format());
    let (target, format, (out_width, out_height)) = if alpha {
        (Pixel::RGBA, FrameFormat::Rgba, fit_within(width, height, MAX_FRAME))
    } else if fits {
        (Pixel::YUV420P, FrameFormat::I420, (width, height))
    } else {
        (Pixel::YUV420P, FrameFormat::I420, fit_within(width, height, MAX_FRAME))
    };
    let key = (source.format(), width, height, target);
    if video.scaler.as_ref().is_none_or(|(_, k)| *k != key) {
        let scaler = ffmpeg::software::scaling::Context::get(
            source.format(),
            width,
            height,
            target,
            out_width,
            out_height,
            ffmpeg::software::scaling::Flags::BILINEAR,
        )?;
        video.scaler = Some((scaler, key));
    }
    let mut converted = ffmpeg::frame::Video::empty();
    video.scaler.as_mut().unwrap().0.run(source, &mut converted)?;
    let (planes, data) = copy_planes(&converted, format.planes());
    // The scaler maps full-range "J" formats to limited range; other inputs keep theirs.
    let jpeg_format = matches!(source.format(), Pixel::YUVJ420P | Pixel::YUVJ422P | Pixel::YUVJ444P | Pixel::YUVJ440P | Pixel::YUVJ411P);
    let full_range = alpha || (declared_full && !jpeg_format);
    Ok(Packed { width: out_width, height: out_height, format, matrix, full_range, planes, data })
}

/// Largest size within `max` with the same aspect ratio, never upscaled, even.
pub fn fit_within(width: u32, height: u32, max: (u32, u32)) -> (u32, u32) {
    let scale = (max.0 as f64 / width as f64).min(max.1 as f64 / height as f64).min(1.0);
    let even = |v: f64| ((v.round() as u32) & !1).max(2);
    (even(width as f64 * scale), even(height as f64 * scale))
}

#[cfg(test)]
mod tests {
    use super::fit_within;

    #[test]
    fn frames_fit_the_output_box_without_upscaling() {
        assert_eq!(fit_within(3840, 2160, (1920, 1080)), (1920, 1080));
        assert_eq!(fit_within(1280, 720, (1920, 1080)), (1280, 720));
        assert_eq!(fit_within(1080, 1920, (1920, 1080)), (608, 1080));
        assert_eq!(fit_within(4096, 1716, (1920, 1080)), (1920, 804));
    }
}
