//! Native child windows inside the operator window, for the live monitors.
//!
//! The operator UI is a webview, which cannot show GPU frames without copying
//! them. Instead each monitor is a borderless child window placed over an
//! empty spot in the UI, and `render_engine` presents into it directly. The UI
//! reports where that spot is (`set_output_monitor`) and hides the monitor
//! while a dialog could overlap it: HTML cannot draw over a native window.
//!
//! Windows: a `WS_CHILD` window above the WebView2 window. Linux: an X11
//! child window (outputs already run through XWayland). Elsewhere live
//! monitors are unavailable and the UI keeps its still previews.

use raw_window_handle::{DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle};

/// A rectangle in the operator window's client area, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Raw handles of a child window, handed to wgpu. The window outlives the
/// surface: it is destroyed only after the render engine has shut down.
pub struct ChildTarget {
    window: RawWindowHandle,
    display: RawDisplayHandle,
}

// SAFETY: the handles are plain identifiers (an HWND or an X11 window id and
// display pointer) that wgpu may use from its presentation thread, as it does
// for the output windows.
unsafe impl Send for ChildTarget {}
unsafe impl Sync for ChildTarget {}

impl HasWindowHandle for ChildTarget {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: the window stays alive for as long as its surface (see above).
        Ok(unsafe { WindowHandle::borrow_raw(self.window) })
    }
}

impl HasDisplayHandle for ChildTarget {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        // SAFETY: the display is the operator window's, open for the app's life.
        Ok(unsafe { DisplayHandle::borrow_raw(self.display) })
    }
}

pub use platform::ChildWindow;

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use render_engine::{MonitorSpec, OutputRole};
use tauri::Manager;

use crate::native_output::NativeOutputs;

/// The monitor windows, created on first use. Main thread only.
#[derive(Default)]
pub struct Monitors {
    windows: Mutex<HashMap<OutputRole, ChildWindow>>,
    /// Roles whose monitor could not start; the UI keeps still previews.
    failed: Mutex<HashSet<OutputRole>>,
}

impl Monitors {
    /// Shows `role`'s monitor at `rect` or hides it. Returns false when live
    /// monitors are unavailable. Must run on the main thread.
    pub fn place<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>, role: OutputRole, rect: Option<Rect>) -> bool {
        if self.failed.lock().expect("monitors poisoned").contains(&role) {
            return false;
        }
        let Some(main) = app.get_webview_window("main") else { return false };
        let Some(outputs) = app.try_state::<NativeOutputs>() else { return false };
        let mut windows = self.windows.lock().expect("monitors poisoned");
        if !windows.contains_key(&role) {
            if rect.is_none() {
                return true;
            }
            match create(&main, &outputs, role) {
                Ok(window) => {
                    log::info!("Live {} monitor started", role.label());
                    windows.insert(role, window);
                }
                Err(e) => {
                    log::warn!("Live {} monitor unavailable, keeping still previews: {e:#}", role.label());
                    self.failed.lock().expect("monitors poisoned").insert(role);
                    return false;
                }
            }
        }
        let window = windows.get_mut(&role).expect("monitor window");
        window.place(rect, &main);
        outputs.set_monitor(role, rect.map(|r| (r.width, r.height)));
        true
    }

    /// Destroys the windows. Call after the render engine has shut down, so
    /// no surface outlives its window.
    pub fn destroy(&self) {
        if let Ok(mut windows) = self.windows.lock() {
            for (_, window) in windows.drain() {
                window.destroy();
            }
        }
    }
}

fn create<R: tauri::Runtime>(main: &tauri::WebviewWindow<R>, outputs: &NativeOutputs, role: OutputRole) -> anyhow::Result<ChildWindow> {
    let parent = main.window_handle()?.as_raw();
    let (window, target) = ChildWindow::new(parent, main)?;
    if let Err(e) = outputs.add_monitor(MonitorSpec { role, target: Box::new(target) }) {
        window.destroy();
        return Err(e);
    }
    Ok(window)
}

#[cfg(windows)]
mod platform {
    use std::num::NonZeroIsize;

    use raw_window_handle::{RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SetWindowPos, ShowWindow, CS_HREDRAW, CS_VREDRAW, HTTRANSPARENT, HWND_TOP,
        SWP_NOACTIVATE, SWP_NOCOPYBITS, SWP_SHOWWINDOW, SW_HIDE, WM_ERASEBKGND, WM_NCHITTEST, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS, WS_DISABLED,
    };

    use super::{ChildTarget, Rect};

    pub struct ChildWindow {
        hwnd: HWND,
        shown: Option<Rect>,
    }

    // The window is only touched on the main thread; the struct lives in
    // Tauri state, which requires Send.
    unsafe impl Send for ChildWindow {}

    const CLASS: &[u16] = &[b'P' as u16, b'A' as u16, b'L' as u16, b'T' as u16, b'O' as u16, b'M' as u16, b'o' as u16, b'n' as u16, 0];

    unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match message {
            // Monitors are display-only: never a click or focus target.
            WM_NCHITTEST => HTTRANSPARENT as LRESULT,
            // The GPU paints every pixel; erasing would flash.
            WM_ERASEBKGND => 1,
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    impl ChildWindow {
        /// Creates a hidden child of `parent`. Call on the main thread.
        pub fn new<R: tauri::Runtime>(parent: RawWindowHandle, _parent_window: &tauri::WebviewWindow<R>) -> anyhow::Result<(Self, ChildTarget)> {
            let RawWindowHandle::Win32(parent) = parent else { anyhow::bail!("The operator window is not a Win32 window") };
            unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                let class = WNDCLASSW {
                    style: CS_HREDRAW | CS_VREDRAW,
                    lpfnWndProc: Some(window_proc),
                    hInstance: instance,
                    lpszClassName: CLASS.as_ptr(),
                    ..std::mem::zeroed()
                };
                // Fails harmlessly when the class already exists.
                RegisterClassW(&class);
                let hwnd = CreateWindowExW(
                    0,
                    CLASS.as_ptr(),
                    std::ptr::null(),
                    WS_CHILD | WS_CLIPSIBLINGS | WS_DISABLED,
                    0,
                    0,
                    1,
                    1,
                    parent.hwnd.get() as HWND,
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null(),
                );
                anyhow::ensure!(!hwnd.is_null(), "Creating the monitor window failed: {}", std::io::Error::last_os_error());
                let mut handle = Win32WindowHandle::new(NonZeroIsize::new(hwnd as isize).expect("non-null hwnd"));
                handle.hinstance = NonZeroIsize::new(instance as isize);
                let target = ChildTarget { window: RawWindowHandle::Win32(handle), display: RawDisplayHandle::Windows(WindowsDisplayHandle::new()) };
                Ok((Self { hwnd, shown: None }, target))
            }
        }

        /// Shows the window at `rect`, above the webview, or hides it.
        pub fn place<R: tauri::Runtime>(&mut self, rect: Option<Rect>, _parent_window: &tauri::WebviewWindow<R>) {
            if rect == self.shown {
                return;
            }
            unsafe {
                match rect {
                    Some(r) => {
                        // No copied bits: the old frame would be blitted to the new
                        // spot until the monitor presents at the new size.
                        SetWindowPos(self.hwnd, HWND_TOP, r.x, r.y, r.width as i32, r.height as i32, SWP_NOACTIVATE | SWP_NOCOPYBITS | SWP_SHOWWINDOW);
                    }
                    None => {
                        ShowWindow(self.hwnd, SW_HIDE);
                    }
                }
            }
            self.shown = rect;
        }

        pub fn destroy(self) {
            unsafe {
                DestroyWindow(self.hwnd);
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use raw_window_handle::{RawDisplayHandle, RawWindowHandle, XlibWindowHandle};
    use x11rb::connection::Connection;
    use x11rb::protocol::shape::{self, ConnectionExt as _};
    use x11rb::protocol::xproto::{ClipOrdering, ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, StackMode, WindowClass};
    use x11rb::rust_connection::RustConnection;
    use x11rb::COPY_FROM_PARENT;

    use super::{ChildTarget, Rect};

    pub struct ChildWindow {
        conn: RustConnection,
        window: u32,
        shown: Option<Rect>,
    }

    impl ChildWindow {
        /// Creates an unmapped X11 child of the operator window. Window ids
        /// are server-wide, so this connection creates and moves the window
        /// while wgpu presents through the operator window's own display.
        pub fn new<R: tauri::Runtime>(parent: RawWindowHandle, parent_window: &tauri::WebviewWindow<R>) -> anyhow::Result<(Self, ChildTarget)> {
            let RawWindowHandle::Xlib(parent) = parent else { anyhow::bail!("Live monitors need the operator window on X11 (XWayland)") };
            let display = raw_window_handle::HasDisplayHandle::display_handle(parent_window)?.as_raw();
            let (conn, _) = RustConnection::connect(None)?;
            let window = conn.generate_id()?;
            // No background: the server never clears it, so moves do not flash.
            conn.create_window(
                COPY_FROM_PARENT as u8,
                window,
                parent.window as u32,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                COPY_FROM_PARENT,
                &CreateWindowAux::new(),
            )?;
            // An empty input region: clicks reach the operator UI underneath.
            conn.shape_rectangles(shape::SO::SET, shape::SK::INPUT, ClipOrdering::UNSORTED, window, 0, 0, &[])?;
            conn.flush()?;
            let target = ChildTarget {
                window: RawWindowHandle::Xlib(XlibWindowHandle::new(window.into())),
                display: match display {
                    RawDisplayHandle::Xlib(display) => RawDisplayHandle::Xlib(display),
                    other => anyhow::bail!("Unexpected operator display handle {other:?}"),
                },
            };
            Ok((Self { conn, window, shown: None }, target))
        }

        /// Maps the window at `rect` (relative to the webview), above it, or
        /// unmaps it. Must run on the main thread (reads GTK geometry).
        pub fn place<R: tauri::Runtime>(&mut self, rect: Option<Rect>, parent_window: &tauri::WebviewWindow<R>) {
            let rect = rect.map(|r| offset_by_webview(r, parent_window));
            if rect == self.shown {
                return;
            }
            let result = match rect {
                Some(r) => self
                    .conn
                    .configure_window(
                        self.window,
                        &ConfigureWindowAux::new().x(r.x).y(r.y).width(r.width.max(1)).height(r.height.max(1)).stack_mode(StackMode::ABOVE),
                    )
                    .and_then(|_| self.conn.map_window(self.window))
                    .map(|_| ()),
                None => self.conn.unmap_window(self.window).map(|_| ()),
            };
            if let Err(e) = result.map_err(anyhow::Error::from).and_then(|()| self.conn.flush().map_err(anyhow::Error::from)) {
                log::warn!("Placing a monitor window: {e}");
            }
            self.shown = rect;
        }

        pub fn destroy(self) {
            let _ = self.conn.destroy_window(self.window);
            let _ = self.conn.flush();
        }
    }

    /// The webview starts below anything GTK puts above it (and inside any
    /// client-side decoration margin), so UI coordinates are shifted by the
    /// webview box's position in the toplevel's X window.
    fn offset_by_webview<R: tauri::Runtime>(rect: Rect, parent_window: &tauri::WebviewWindow<R>) -> Rect {
        use gtk::prelude::WidgetExt;
        let Ok(vbox) = parent_window.default_vbox() else { return rect };
        let allocation = vbox.allocation();
        let scale = vbox.scale_factor();
        Rect { x: rect.x + allocation.x() * scale, y: rect.y + allocation.y() * scale, ..rect }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::{ChildTarget, Rect};
    use raw_window_handle::RawWindowHandle;

    pub struct ChildWindow;

    impl ChildWindow {
        pub fn new<R: tauri::Runtime>(_parent: RawWindowHandle, _parent_window: &tauri::WebviewWindow<R>) -> anyhow::Result<(Self, ChildTarget)> {
            anyhow::bail!("Live monitors are not available on this system")
        }

        pub fn place<R: tauri::Runtime>(&mut self, _rect: Option<Rect>, _parent_window: &tauri::WebviewWindow<R>) {}

        pub fn destroy(self) {}
    }
}
