use raw_window_handle::{DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub struct ChildTarget {
    window: RawWindowHandle,
    display: RawDisplayHandle,
}

unsafe impl Send for ChildTarget {}
unsafe impl Sync for ChildTarget {}

impl HasWindowHandle for ChildTarget {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Ok(unsafe { WindowHandle::borrow_raw(self.window) })
    }
}

impl HasDisplayHandle for ChildTarget {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(self.display) })
    }
}

pub use platform::ChildWindow;

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use render_engine::{MonitorSpec, OutputRole};
use tauri::Manager;

use crate::output::NativeOutputs;

#[derive(Default)]
pub struct Monitors {
    windows: Mutex<HashMap<OutputRole, ChildWindow>>,
    failed: Mutex<HashSet<OutputRole>>,
}

impl Monitors {
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

    unsafe impl Send for ChildWindow {}

    const CLASS: &[u16] = &[b'P' as u16, b'A' as u16, b'L' as u16, b'T' as u16, b'O' as u16, b'M' as u16, b'o' as u16, b'n' as u16, 0];

    unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match message {
            WM_NCHITTEST => HTTRANSPARENT as LRESULT,
            WM_ERASEBKGND => 1,
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }

    impl ChildWindow {
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

        pub fn place<R: tauri::Runtime>(&mut self, rect: Option<Rect>, _parent_window: &tauri::WebviewWindow<R>) {
            if rect == self.shown {
                return;
            }
            unsafe {
                match rect {
                    Some(r) => {
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
        pub fn new<R: tauri::Runtime>(parent: RawWindowHandle, parent_window: &tauri::WebviewWindow<R>) -> anyhow::Result<(Self, ChildTarget)> {
            let RawWindowHandle::Xlib(parent) = parent else { anyhow::bail!("Live monitors need the operator window on X11 (XWayland)") };
            let display = raw_window_handle::HasDisplayHandle::display_handle(parent_window)?.as_raw();
            let (conn, _) = RustConnection::connect(None)?;
            let window = conn.generate_id()?;
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
