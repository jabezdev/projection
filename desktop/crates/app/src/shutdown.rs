use std::sync::Arc;
use std::time::Duration;

use tauri::Manager;

use crate::output::media::NativeMedia;
use crate::output::NativeOutputs;
use crate::{capture, remote, Storage};
use crate::output::monitor_window;
use crate::system::awake;
use crate::system::recovery;

const PENDING_SAVES_TIMEOUT: Duration = Duration::from_secs(5);

/// Writes pending saves, then stops presentation before the decoders it reads
/// from. Safe to call more than once.
pub(crate) fn shutdown_native<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(store) = app.try_state::<Storage>() {
        if !store.flush(PENDING_SAVES_TIMEOUT) {
            log::error!("Exiting with project saves still pending");
        }
    }
    if let Some(outputs) = app.try_state::<NativeOutputs>() {
        outputs.shutdown();
    }
    if let Some(monitors) = app.try_state::<monitor_window::Monitors>() {
        monitors.destroy();
    }
    if let Some(awake) = app.try_state::<awake::KeepAwake>() {
        awake.set(false);
    }
    if let Some(media) = app.try_state::<Arc<NativeMedia>>() {
        media.shutdown();
    }
    if let Some(captures) = app.try_state::<Arc<capture::Captures>>() {
        captures.shutdown();
    }
    if let Some(recovery) = app.try_state::<Arc<recovery::Recovery>>() {
        recovery.clean_exit();
    }
    if let Some(remote) = app.try_state::<remote::RemoteState>() {
        remote.stop_advertising();
    }
}
