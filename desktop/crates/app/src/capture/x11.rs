//! X11 capture: whole monitors (RandR) and single windows (Composite, so a
//! covered window still captures), read with GetImage about 30 times a
//! second. Simple and dependable but heavier than the portal; it only sees
//! X11 windows.

use std::time::{Duration, Instant};

use protocol::{CaptureKind, CaptureSource};
use x11rb::connection::Connection;
use x11rb::protocol::composite::{self, ConnectionExt as _};
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, ImageFormat, Window};
use x11rb::rust_connection::RustConnection;

use super::{Backend, CaptureStream, FrameSlot};

const INTERVAL: Duration = Duration::from_millis(33);

pub struct X11;

fn atom(conn: &RustConnection, name: &str) -> anyhow::Result<u32> {
    Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
}

fn text_property(conn: &RustConnection, window: Window, property: u32, kind: u32) -> Option<String> {
    let reply = conn.get_property(false, window, property, kind, 0, 1024).ok()?.reply().ok()?;
    (!reply.value.is_empty()).then(|| String::from_utf8_lossy(&reply.value).into_owned())
}

/// A window's title and program (`WM_CLASS` class).
fn window_names(conn: &RustConnection, window: Window) -> (String, String) {
    let utf8 = atom(conn, "UTF8_STRING").unwrap_or(AtomEnum::STRING.into());
    let net_name = atom(conn, "_NET_WM_NAME").ok();
    let title = net_name
        .and_then(|a| text_property(conn, window, a, utf8))
        .or_else(|| text_property(conn, window, AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()))
        .unwrap_or_default();
    let class = text_property(conn, window, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into())
        .and_then(|c| c.split('\0').nth(1).map(str::to_string))
        .unwrap_or_default();
    (title, class)
}

fn windows(conn: &RustConnection, root: Window) -> anyhow::Result<Vec<Window>> {
    let list = atom(conn, "_NET_CLIENT_LIST")?;
    let reply = conn.get_property(false, root, list, AtomEnum::WINDOW, 0, 4096)?.reply()?;
    Ok(reply.value32().map(|v| v.collect()).unwrap_or_default())
}

/// What a source id names, found again on this display.
enum Target {
    /// Root-window rectangle of a monitor.
    Monitor { x: i16, y: i16, width: u16, height: u16 },
    Window(Window),
}

fn find(conn: &RustConnection, root: Window, source: &CaptureSource) -> anyhow::Result<Target> {
    if let Some(name) = source.id.strip_prefix("x11-monitor:") {
        let monitors = conn.randr_get_monitors(root, true)?.reply()?;
        for monitor in monitors.monitors {
            let monitor_name = String::from_utf8_lossy(&conn.get_atom_name(monitor.name)?.reply()?.name).into_owned();
            if monitor_name == name {
                return Ok(Target::Monitor { x: monitor.x, y: monitor.y, width: monitor.width, height: monitor.height });
            }
        }
        anyhow::bail!("The display {name} is not connected");
    }
    if let Some(key) = source.id.strip_prefix("x11-window:") {
        for window in windows(conn, root)? {
            let (title, class) = window_names(conn, window);
            if format!("{class}|{title}") == key {
                return Ok(Target::Window(window));
            }
        }
        anyhow::bail!("The window \"{}\" is not open", source.name);
    }
    anyhow::bail!("This source belongs to another capture method")
}

impl Backend for X11 {
    fn name(&self) -> &'static str {
        "x11"
    }

    fn label(&self) -> &'static str {
        "X11"
    }

    fn sources(&self) -> anyhow::Result<Vec<CaptureSource>> {
        let (conn, screen) = RustConnection::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let mut sources = Vec::new();
        for monitor in conn.randr_get_monitors(root, true)?.reply()?.monitors {
            let name = String::from_utf8_lossy(&conn.get_atom_name(monitor.name)?.reply()?.name).into_owned();
            sources.push(CaptureSource { kind: CaptureKind::Screen, id: format!("x11-monitor:{name}"), name: format!("{name} ({}×{})", monitor.width, monitor.height) });
        }
        for window in windows(&conn, root)? {
            let (title, class) = window_names(&conn, window);
            if title.is_empty() {
                continue;
            }
            let name = if class.is_empty() { title.clone() } else { format!("{title} — {class}") };
            sources.push(CaptureSource { kind: CaptureKind::Window, id: format!("x11-window:{class}|{title}"), name });
        }
        Ok(sources)
    }

    fn start(&self, source: &CaptureSource, slot: FrameSlot) -> anyhow::Result<Box<dyn CaptureStream>> {
        let (conn, screen) = RustConnection::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let target = find(&conn, root, source)?;
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let keep = running.clone();
        std::thread::Builder::new().name("capture-x11".into()).spawn(move || {
            if let Err(e) = capture(&conn, root, target, &slot, &keep) {
                slot.lose(format!("{e:#}"));
            }
        })?;
        Ok(Box::new(Running(running)))
    }
}

struct Running(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl CaptureStream for Running {}

impl Drop for Running {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

fn capture(conn: &RustConnection, root: Window, target: Target, slot: &FrameSlot, running: &std::sync::atomic::AtomicBool) -> anyhow::Result<()> {
    if let Target::Window(window) = target {
        conn.composite_query_version(0, 4)?.reply()?;
        conn.composite_redirect_window(window, composite::Redirect::AUTOMATIC)?.check()?;
    }
    let mut pixmap: Option<(u32, (u16, u16))> = None;
    while running.load(std::sync::atomic::Ordering::Acquire) {
        let started = Instant::now();
        let (drawable, x, y, width, height) = match target {
            Target::Monitor { x, y, width, height } => (root, x, y, width, height),
            Target::Window(window) => {
                let geometry = match conn.get_geometry(window)?.reply() {
                    Ok(geometry) => geometry,
                    Err(_) => anyhow::bail!("The window was closed"),
                };
                let size = (geometry.width, geometry.height);
                // The window's off-screen picture, renamed whenever it resizes.
                if pixmap.is_none_or(|(_, s)| s != size) {
                    if let Some((old, _)) = pixmap.take() {
                        let _ = conn.free_pixmap(old);
                    }
                    let id = conn.generate_id()?;
                    conn.composite_name_window_pixmap(window, id)?.check()?;
                    pixmap = Some((id, size));
                }
                (pixmap.expect("named above").0, 0, 0, size.0, size.1)
            }
        };
        let image = conn.get_image(ImageFormat::Z_PIXMAP, drawable, x, y, width, height, !0)?.reply();
        match image {
            // 24- and 32-bit visuals arrive as BGRx rows.
            Ok(image) if image.depth >= 24 => slot.publish(width as u32, height as u32, width as usize * 4, image.data),
            Ok(image) => anyhow::bail!("Unsupported color depth {}", image.depth),
            Err(_) if matches!(target, Target::Window(_)) => anyhow::bail!("The window was closed"),
            Err(_) if std::env::var_os("WAYLAND_DISPLAY").is_some() => {
                anyhow::bail!("X11 cannot read the screen under Wayland; choose the screen sharing portal in Settings → System")
            }
            Err(e) => return Err(e.into()),
        }
        std::thread::sleep(INTERVAL.saturating_sub(started.elapsed()));
    }
    if let Some((id, _)) = pixmap {
        let _ = conn.free_pixmap(id);
    }
    Ok(())
}

