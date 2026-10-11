//! Still thumbnails of video files for the operator UI, decoded in-process so
//! a packaged app needs no FFmpeg command-line tool.

use anyhow::Context;
use ffmpeg_next as ffmpeg;

/// Gives up on files whose video stream yields no frame this early.
const PACKET_LIMIT: usize = 4096;

/// One video frame as tightly packed RGB.
pub struct Poster {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// The first decodable frame of `path`, scaled down to at most `max_width`
/// pixels wide (never up), with an even height.
pub fn poster(path: &str, max_width: u32) -> anyhow::Result<Poster> {
    ffmpeg::init()?;
    let mut input = ffmpeg::format::input(&path).with_context(|| format!("Opening {path}"))?;
    let stream = input.streams().best(ffmpeg::media::Type::Video).context("The file has no video stream")?;
    let index = stream.index();
    let mut decoder = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?.decoder().video()?;
    let mut decoded = ffmpeg::frame::Video::empty();
    let mut packets = 0;
    for (stream, packet) in input.packets() {
        if stream.index() != index {
            continue;
        }
        packets += 1;
        anyhow::ensure!(packets <= PACKET_LIMIT, "No video frame could be decoded");
        // A damaged packet early in a file is skipped, as a player would.
        if decoder.send_packet(&packet).is_err() {
            continue;
        }
        if decoder.receive_frame(&mut decoded).is_ok() {
            return convert(&decoded, max_width);
        }
    }
    decoder.send_eof()?;
    decoder.receive_frame(&mut decoded).context("No video frame could be decoded")?;
    convert(&decoded, max_width)
}

fn convert(decoded: &ffmpeg::frame::Video, max_width: u32) -> anyhow::Result<Poster> {
    let (source_width, source_height) = (decoded.width(), decoded.height());
    anyhow::ensure!(source_width > 0 && source_height > 0, "The video has no picture size");
    let width = source_width.min(max_width.max(2)) & !1;
    let height = ((u64::from(source_height) * u64::from(width) / u64::from(source_width)).max(2) as u32) & !1;
    let mut scaler = ffmpeg::software::scaling::Context::get(
        decoded.format(),
        source_width,
        source_height,
        ffmpeg::format::Pixel::RGB24,
        width,
        height,
        ffmpeg::software::scaling::Flags::BILINEAR,
    )?;
    let mut rgb = ffmpeg::frame::Video::empty();
    scaler.run(decoded, &mut rgb)?;
    let stride = rgb.stride(0);
    let row = width as usize * 3;
    let data = rgb.data(0);
    let pixels = (0..height as usize).flat_map(|y| &data[y * stride..y * stride + row]).copied().collect();
    Ok(Poster { width, height, rgb: pixels })
}
