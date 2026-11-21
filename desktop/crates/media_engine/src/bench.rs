use std::time::{Duration, Instant};

use anyhow::Context;
use ffmpeg_next as ffmpeg;

use crate::decoder::decode_threads;
use crate::hwaccel;

const LUMA_GRID: (usize, usize) = (64, 36);
const SAMPLE_FRAME: u32 = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct DecodeRun {
    pub decoder: String,
    pub codec: String,
    pub size: (u32, u32),
    pub pixel_format: String,
    pub frames: u32,
    pub video_seconds: f64,
    pub wall: Duration,
    pub luma: Option<Vec<u8>>,
}

impl DecodeRun {
    pub fn realtime_factor(&self) -> f64 {
        self.video_seconds / self.wall.as_secs_f64().max(1e-6)
    }

    pub fn fps(&self) -> f64 {
        self.frames as f64 / self.wall.as_secs_f64().max(1e-6)
    }
}

pub fn luma_difference(a: &DecodeRun, b: &DecodeRun) -> Option<f64> {
    let (a, b) = (a.luma.as_ref()?, b.luma.as_ref()?);
    (a.len() == b.len() && !a.is_empty())
        .then(|| a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).abs()).sum::<f64>() / a.len() as f64)
}

pub fn decode_run(path: &str, device: Option<&'static str>, max_frames: u32, max_wall: Duration, stop: &dyn Fn() -> bool) -> anyhow::Result<DecodeRun> {
    ffmpeg::init()?;
    let mut input = ffmpeg::format::input(path).with_context(|| format!("Opening {path}"))?;
    let stream = input.streams().best(ffmpeg::media::Type::Video).context("No video stream")?;
    let index = stream.index();
    let time_base = f64::from(stream.time_base());
    let frame_rate = f64::from(stream.avg_frame_rate()).max(1.0);
    let mut context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?;
    let codec = ffmpeg::decoder::find(context.id()).context("No decoder for this video")?;
    let hw = match device {
        Some(name) => Some(hwaccel::attach_first(&mut context, unsafe { codec.as_ptr() }, &[name]).context("Not supported for this video")?),
        None => {
            context.set_threading(ffmpeg::codec::threading::Config {
                kind: ffmpeg::codec::threading::Type::Frame,
                count: decode_threads(),
                ..Default::default()
            });
            None
        }
    };
    let mut decoder = context.decoder().video()?;
    let mut run = DecodeRun {
        decoder: device.unwrap_or("software").to_string(),
        codec: codec.name().to_string(),
        size: (decoder.width(), decoder.height()),
        pixel_format: format!("{:?}", decoder.format()),
        frames: 0,
        video_seconds: 0.0,
        wall: Duration::ZERO,
        luma: None,
    };
    let (mut first_pts, mut last_pts) = (None, None);
    let mut decoded = ffmpeg::frame::Video::empty();
    let started = Instant::now();
    let mut receive = |decoder: &mut ffmpeg::decoder::Video, run: &mut DecodeRun| -> anyhow::Result<()> {
        while decoder.receive_frame(&mut decoded).is_ok() {
            let on_gpu = hw.is_some_and(|hw| ffmpeg::ffi::AVPixelFormat::from(decoded.format()) == hw.format);
            let frame = if on_gpu { hwaccel::download(&decoded).context("Copying a frame from the GPU")? } else { decoded.clone() };
            if let Some(pts) = frame.timestamp().or(frame.pts()) {
                first_pts.get_or_insert(pts);
                last_pts = Some(pts);
            }
            run.frames += 1;
            if run.frames == SAMPLE_FRAME {
                run.luma = sample_luma(&frame);
            }
        }
        Ok(())
    };
    for (stream, packet) in input.packets() {
        if run.frames >= max_frames || started.elapsed() >= max_wall || stop() {
            break;
        }
        if stream.index() != index {
            continue;
        }
        decoder.send_packet(&packet)?;
        receive(&mut decoder, &mut run)?;
    }
    if run.frames < max_frames {
        decoder.send_eof()?;
        receive(&mut decoder, &mut run)?;
    }
    run.wall = started.elapsed();
    run.video_seconds = match (first_pts, last_pts) {
        (Some(first), Some(last)) if last > first => (last - first) as f64 * time_base + 1.0 / frame_rate,
        _ => run.frames as f64 / frame_rate,
    };
    anyhow::ensure!(run.frames > 0, "No frames decoded");
    Ok(run)
}

fn sample_luma(frame: &ffmpeg::frame::Video) -> Option<Vec<u8>> {
    use ffmpeg::format::Pixel;
    if !matches!(frame.format(), Pixel::YUV420P | Pixel::YUVJ420P | Pixel::NV12) {
        return None;
    }
    let (width, height, stride) = (frame.width() as usize, frame.height() as usize, frame.stride(0));
    let data = frame.data(0);
    let (columns, rows) = LUMA_GRID;
    Some(
        (0..rows)
            .flat_map(|row| (0..columns).map(move |column| ((row * 2 + 1) * height / (rows * 2), (column * 2 + 1) * width / (columns * 2))))
            .map(|(y, x)| data[y * stride + x])
            .collect(),
    )
}
