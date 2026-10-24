//! Wayland capture through the xdg screen-cast portal and PipeWire. The
//! system's own dialog picks the screen or window once; the portal's
//! restore token then reopens it without asking. Tokens are single use, so
//! each source keeps a stable id and its newest token is kept in
//! `capture-tokens.json`.

use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
use ashpd::desktop::PersistMode;
use pipewire as pw;
use protocol::{CaptureKind, CaptureSource};
use pw::spa;

use super::{Backend, CaptureStream, FrameSlot};

const TOKENS_FILE: &str = "capture-tokens.json";

pub struct Portal {
    tokens: Arc<Mutex<HashMap<String, String>>>,
    path: Option<PathBuf>,
}

impl Portal {
    pub fn new(data_dir: Option<&std::path::Path>) -> Self {
        let path = data_dir.map(|dir| dir.join(TOKENS_FILE));
        let tokens = path.as_ref().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { tokens: Arc::new(Mutex::new(tokens)), path }
    }
}

fn save_tokens(path: &Option<PathBuf>, tokens: &HashMap<String, String>) {
    if let Some(path) = path {
        if let Err(e) = crate::settings::write_atomic(path, tokens) {
            log::warn!("Saving capture tokens: {e}");
        }
    }
}

fn source_types(kind: CaptureKind) -> SourceType {
    match kind {
        CaptureKind::Screen => SourceType::Monitor,
        CaptureKind::Window => SourceType::Window,
    }
}

/// Opens a screen-cast session (showing the system dialog unless `token`
/// restores the previous choice). Returns the PipeWire node, its remote and
/// the next restore token; the session stays open while `session` lives.
async fn open(kind: CaptureKind, token: Option<&str>) -> anyhow::Result<(u32, (i32, i32), OwnedFd, Option<String>, ashpd::desktop::Session<'static, Screencast<'static>>)> {
    let proxy = Screencast::new().await?;
    let session = proxy.create_session().await?;
    proxy
        .select_sources(&session, CursorMode::Embedded, source_types(kind).into(), false, token, PersistMode::ExplicitlyRevoked)
        .await?;
    let response = proxy.start(&session, None).await?.response()?;
    let stream = response.streams().first().cloned().ok_or_else(|| anyhow::anyhow!("Nothing was chosen to share"))?;
    let fd = proxy.open_pipe_wire_remote(&session).await?;
    Ok((stream.pipe_wire_node_id(), stream.size().unwrap_or((0, 0)), fd, response.restore_token().map(str::to_string), session))
}

impl Backend for Portal {
    fn name(&self) -> &'static str {
        "portal"
    }

    fn label(&self) -> &'static str {
        "Screen sharing portal (PipeWire)"
    }

    fn system_picker(&self) -> bool {
        true
    }

    fn sources(&self) -> anyhow::Result<Vec<CaptureSource>> {
        Ok(Vec::new())
    }

    fn pick(&self, kind: CaptureKind) -> anyhow::Result<Option<CaptureSource>> {
        let result = tauri::async_runtime::block_on(async move {
            let (_, size, _, token, session) = open(kind, None).await?;
            let _ = session.close().await;
            anyhow::Ok((size, token))
        });
        let (size, token) = match result {
            Ok(picked) => picked,
            Err(e) if e.downcast_ref::<ashpd::Error>().is_some_and(|e| matches!(e, ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled))) => return Ok(None),
            Err(e) => return Err(e),
        };
        let id = format!("portal:{}", uuid::Uuid::new_v4());
        if let Some(token) = token {
            let mut tokens = self.tokens.lock().expect("capture tokens poisoned");
            tokens.insert(id.clone(), token);
            save_tokens(&self.path, &tokens);
        }
        let what = match kind {
            CaptureKind::Screen => "Screen",
            CaptureKind::Window => "Window",
        };
        let name = if size.0 > 0 { format!("{what} ({}×{})", size.0, size.1) } else { what.to_string() };
        Ok(Some(CaptureSource { kind, id, name }))
    }

    fn start(&self, source: &CaptureSource, slot: FrameSlot) -> anyhow::Result<Box<dyn CaptureStream>> {
        let (stop, stopped) = pw::channel::channel::<()>();
        let (tokens, path, source) = (self.tokens.clone(), self.path.clone(), source.clone());
        std::thread::Builder::new().name("capture-portal".into()).spawn(move || {
            let token = tokens.lock().expect("capture tokens poisoned").get(&source.id).cloned();
            let opened = tauri::async_runtime::block_on(open(source.kind, token.as_deref()));
            let (node, _, fd, next, session) = match opened {
                Ok(opened) => opened,
                Err(e) => return slot.lose(format!("Screen sharing did not start: {e}")),
            };
            if let Some(next) = next {
                let mut tokens = tokens.lock().expect("capture tokens poisoned");
                tokens.insert(source.id.clone(), next);
                save_tokens(&path, &tokens);
            }
            if let Err(e) = stream(node, fd, &slot, stopped) {
                slot.lose(format!("Screen sharing failed: {e}"));
            }
            let _ = tauri::async_runtime::block_on(session.close());
        })?;
        Ok(Box::new(Running(Some(stop))))
    }
}

struct Running(Option<pw::channel::Sender<()>>);

impl CaptureStream for Running {}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(stop) = self.0.take() {
            let _ = stop.send(());
        }
    }
}

#[derive(Default)]
struct Format {
    info: spa::param::video::VideoInfoRaw,
}

/// Receives frames from PipeWire node `node` until `stopped` fires or the
/// stream ends.
fn stream(node: u32, fd: OwnedFd, slot: &FrameSlot, stopped: pw::channel::Receiver<()>) -> anyhow::Result<()> {
    use spa::param::video::VideoFormat;
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_fd_rc(fd, None)?;
    let stream = pw::stream::StreamRc::new(
        core,
        "PALTO capture",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;
    let stop_loop = mainloop.clone();
    let _stop = stopped.attach(mainloop.loop_(), move |()| stop_loop.quit());
    let ended = slot.clone();
    let frames = slot.clone();
    let quit = mainloop.clone();
    let _listener = stream
        .add_local_listener_with_user_data(Format::default())
        .state_changed(move |_, _, _, state| match state {
            pw::stream::StreamState::Error(e) => {
                ended.lose(format!("Screen sharing stopped: {e}"));
                quit.quit();
            }
            pw::stream::StreamState::Unconnected => {
                ended.lose("Screen sharing stopped".into());
                quit.quit();
            }
            _ => {}
        })
        .param_changed(|_, format, id, param| {
            let Some(param) = param.filter(|_| id == spa::param::ParamType::Format.as_raw()) else { return };
            if format.info.parse(param).is_err() {
                log::warn!("Unreadable screen-share format");
            }
        })
        .process(move |stream, format| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let size = format.info.size();
            let video_format = format.info.format();
            let Some(data) = buffer.datas_mut().first_mut() else { return };
            let (offset, stride) = (data.chunk().offset() as usize, data.chunk().stride().max(0) as usize);
            let Some(bytes) = data.data() else { return };
            let (width, height) = (size.width, size.height);
            let stride = if stride == 0 { width as usize * 4 } else { stride };
            let Some(pixels) = bytes.get(offset..offset + stride * height as usize) else { return };
            let mut copy = pixels.to_vec();
            if matches!(video_format, VideoFormat::RGBx | VideoFormat::RGBA) {
                // The outputs take BGRA; swap red and blue.
                for row in copy.chunks_exact_mut(stride) {
                    for pixel in row[..width as usize * 4].chunks_exact_mut(4) {
                        pixel.swap(0, 2);
                    }
                }
            }
            frames.publish(width, height, stride, copy);
        })
        .register()?;

    let format = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(spa::param::format::FormatProperties::MediaType, Id, spa::param::format::MediaType::Video),
        spa::pod::property!(spa::param::format::FormatProperties::MediaSubtype, Id, spa::param::format::MediaSubtype::Raw),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle { width: 1920, height: 1080 },
            spa::utils::Rectangle { width: 1, height: 1 },
            spa::utils::Rectangle { width: 8192, height: 8192 }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction { num: 30, denom: 1 },
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction { num: 60, denom: 1 }
        ),
    );
    let bytes: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &spa::pod::Value::Object(format))?.0.into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&bytes).ok_or_else(|| anyhow::anyhow!("Invalid format request"))?];
    stream.connect(spa::utils::Direction::Input, Some(node), pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS, &mut params)?;
    mainloop.run();
    let _ = stream.disconnect();
    Ok(())
}
