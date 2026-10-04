use ffmpeg_next as ffmpeg;
use crossbeam::channel::{Receiver, unbounded};
use std::thread;
use log::{error, info};

pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub struct VideoPlayer {
    frame_rx: Receiver<VideoFrame>,
}

impl VideoPlayer {
    pub fn new(path: &str) -> anyhow::Result<Self> {
        let (frame_tx, frame_rx) = unbounded();
        let path = path.to_string();

        thread::spawn(move || {
            if let Err(e) = decode_video(&path, frame_tx) {
                error!("Video decode error: {}", e);
            }
        });

        Ok(Self { frame_rx })
    }

    pub fn next_frame(&self) -> Option<VideoFrame> {
        // Drain until we get the most recent frame or empty
        let mut last_frame = None;
        while let Ok(frame) = self.frame_rx.try_recv() {
            last_frame = Some(frame);
        }
        last_frame
    }
}

fn decode_video(path: &str, tx: crossbeam::channel::Sender<VideoFrame>) -> anyhow::Result<()> {
    ffmpeg::init()?;
    
    let mut ictx = ffmpeg::format::input(&path)?;
    let input = ictx
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow::anyhow!("No video stream found"))?;
    
    let video_stream_index = input.index();
    let context_decoder = ffmpeg::codec::context::Context::from_parameters(input.parameters())?;
    let mut decoder = context_decoder.decoder().video()?;

    let mut scaler = ffmpeg::software::scaling::context::Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        ffmpeg::format::Pixel::RGBA,
        decoder.width(),
        decoder.height(),
        ffmpeg::software::scaling::flag::Flags::BILINEAR,
    )?;

    info!("Started decoding video: {}", path);

    
    loop {
        for (stream, packet) in ictx.packets() {
            if stream.index() == video_stream_index {
                decoder.send_packet(&packet)?;
                let mut decoded = ffmpeg::util::frame::video::Video::empty();
                while decoder.receive_frame(&mut decoded).is_ok() {
                    let mut rgba_frame = ffmpeg::util::frame::video::Video::empty();
                    scaler.run(&decoded, &mut rgba_frame)?;

                    let frame = VideoFrame {
                        width: rgba_frame.width(),
                        height: rgba_frame.height(),
                        data: rgba_frame.data(0).to_vec(),
                    };

                    if tx.send(frame).is_err() {
                        return Ok(()); // Receiver dropped
                    }
                    
                    // Simplified: limit to ~30fps
                    thread::sleep(std::time::Duration::from_millis(30));
                }
            }
        }
        
        // Seek back to start for looping
        ictx.seek(0, 0..1000)?;
        decoder.flush();
    }
}
