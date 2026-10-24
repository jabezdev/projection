//! Keeps the displays awake while any output is shown, so a long stretch
//! without input never blanks the projector, starts a screensaver or sleeps.
//!
//! Windows: `SetThreadExecutionState`, which belongs to the calling thread, so
//! one thread owns it for the app's lifetime. Linux: the session's
//! `org.freedesktop.ScreenSaver` idle inhibitor (honored by hypridle, GNOME
//! and KDE), released when outputs are hidden.

use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;

pub struct KeepAwake {
    requests: Mutex<Sender<bool>>,
    held: Mutex<bool>,
}

impl KeepAwake {
    pub fn start() -> anyhow::Result<Self> {
        let (requests, received) = channel::<bool>();
        std::thread::Builder::new().name("keep-awake".into()).spawn(move || {
            let mut inhibitor = platform::Inhibitor::default();
            for awake in received {
                inhibitor.set(awake);
            }
            inhibitor.set(false);
        })?;
        Ok(Self { requests: Mutex::new(requests), held: Mutex::new(false) })
    }

    /// Holds or releases the displays; repeated calls with the same value do nothing.
    pub fn set(&self, awake: bool) {
        let Ok(mut held) = self.held.lock() else { return };
        if *held == awake {
            return;
        }
        *held = awake;
        log::info!("{} the displays awake", if awake { "Keeping" } else { "No longer keeping" });
        if let Ok(requests) = self.requests.lock() {
            let _ = requests.send(awake);
        }
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED};

    #[derive(Default)]
    pub struct Inhibitor;

    impl Inhibitor {
        pub fn set(&mut self, awake: bool) {
            let flags = if awake { ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS };
            // SAFETY: plain Win32 call with valid flags; affects only this thread.
            if unsafe { SetThreadExecutionState(flags) } == 0 {
                log::warn!("SetThreadExecutionState failed; the display may still sleep");
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use gtk::gio;
    use gtk::glib::{self, ToVariant};

    const SERVICE: &str = "org.freedesktop.ScreenSaver";
    const PATH: &str = "/org/freedesktop/ScreenSaver";

    /// The inhibit cookie lives as long as the D-Bus connection that took it.
    #[derive(Default)]
    pub struct Inhibitor {
        held: Option<(gio::DBusConnection, u32)>,
    }

    impl Inhibitor {
        pub fn set(&mut self, awake: bool) {
            match (awake, self.held.take()) {
                (true, None) => match inhibit() {
                    Ok(held) => self.held = Some(held),
                    Err(e) => log::warn!("Cannot inhibit idle through {SERVICE}: {e}; the display may still blank"),
                },
                (false, Some((bus, cookie))) => {
                    let args = (cookie,).to_variant();
                    if let Err(e) = bus.call_sync(Some(SERVICE), PATH, SERVICE, "UnInhibit", Some(&args), None, gio::DBusCallFlags::NONE, 2000, gio::Cancellable::NONE) {
                        log::warn!("Releasing the idle inhibitor failed: {e}");
                    }
                }
                (_, held) => self.held = held,
            }
        }
    }

    fn inhibit() -> Result<(gio::DBusConnection, u32), glib::Error> {
        let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)?;
        let args = ("PALTO", "Projection outputs are shown").to_variant();
        let reply = bus.call_sync(Some(SERVICE), PATH, SERVICE, "Inhibit", Some(&args), Some(glib::VariantTy::new("(u)").expect("valid type")), gio::DBusCallFlags::NONE, 2000, gio::Cancellable::NONE)?;
        let (cookie,) = reply.get::<(u32,)>().unwrap_or((0,));
        Ok((bus, cookie))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    #[derive(Default)]
    pub struct Inhibitor;

    impl Inhibitor {
        pub fn set(&mut self, _awake: bool) {}
    }
}
