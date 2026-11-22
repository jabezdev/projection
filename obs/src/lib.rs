mod connection;
mod data;
pub mod discover;
mod fade;
mod ffi;
mod properties;
mod source;
pub mod text;

use std::ffi::{c_char, CStr};
use std::sync::atomic::{AtomicPtr, Ordering};

use ffi::*;
use source::{LYRICS, SECTION, SONG_INFO};

const OBS_API_VERSION: u32 = 30 << 24;

static MODULE: AtomicPtr<obs_module_t> = AtomicPtr::new(std::ptr::null_mut());

#[no_mangle]
pub extern "C" fn obs_module_set_pointer(module: *mut obs_module_t) {
    MODULE.store(module, Ordering::Relaxed);
}

#[no_mangle]
pub extern "C" fn obs_current_module() -> *mut obs_module_t {
    MODULE.load(Ordering::Relaxed)
}

#[no_mangle]
pub extern "C" fn obs_module_ver() -> u32 {
    OBS_API_VERSION
}

#[no_mangle]
pub extern "C" fn obs_module_name() -> *const c_char {
    c"PALTO".as_ptr()
}

#[no_mangle]
pub extern "C" fn obs_module_description() -> *const c_char {
    c"Shows the lyrics and song credits from PALTO.".as_ptr()
}

fn info<const K: u8>(id: &'static CStr) -> obs_source_info {
    obs_source_info {
        id: id.as_ptr(),
        type_: OBS_SOURCE_TYPE_INPUT,
        output_flags: OBS_SOURCE_VIDEO | OBS_SOURCE_CUSTOM_DRAW | OBS_SOURCE_SRGB,
        get_name: Some(source::name::<K>),
        create: Some(source::create::<K>),
        destroy: Some(source::destroy),
        get_width: Some(source::width),
        get_height: Some(source::height),
        get_defaults: Some(properties::defaults::<K>),
        get_properties: Some(properties::properties::<K>),
        update: Some(source::update),
        activate: None,
        deactivate: None,
        show: None,
        hide: None,
        video_tick: Some(source::video_tick),
        video_render: Some(source::video_render),
        filter_video: None,
        filter_audio: None,
        enum_active_sources: Some(source::children),
        save: None,
        load: None,
        mouse_click: None,
        mouse_move: None,
        mouse_wheel: None,
        focus: None,
        key_click: None,
        filter_remove: None,
        type_data: std::ptr::null_mut(),
        free_type_data: None,
        audio_render: None,
        enum_all_sources: Some(source::children),
        transition_start: None,
        transition_stop: None,
        get_defaults2: None,
        get_properties2: None,
        audio_mix: None,
        icon_type: OBS_ICON_TYPE_TEXT,
    }
}

#[no_mangle]
pub extern "C" fn obs_module_load() -> bool {
    for info in [info::<LYRICS>(c"palto_lyrics"), info::<SONG_INFO>(c"palto_song_info"), info::<SECTION>(c"palto_section")] {
        unsafe { obs_register_source_s(&info, std::mem::size_of::<obs_source_info>()) };
    }
    std::thread::spawn(|| discover::scan_blocking(std::time::Duration::from_secs(10)));
    true
}

#[no_mangle]
pub extern "C" fn obs_module_post_load() {
    let text = source::text_source();
    if unsafe { source::exists(text) } {
        data::log(LOG_INFO, &format!("Loaded {} using {}", env!("CARGO_PKG_VERSION"), text.to_string_lossy()));
    } else {
        data::log(LOG_WARNING, "No OBS text source found; PALTO sources cannot show text");
    }
}
