use std::ffi::{c_char, c_void, CStr};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use crate::connection::{self, Connection, Status};
use crate::data::{self, boolean, int, string};
use crate::fade::Fader;
use crate::ffi::*;
use crate::text::{self, Kind, Options};

unsafe impl Send for Source {}
unsafe impl Sync for Source {}

pub const LYRICS: u8 = 0;
pub const SONG_INFO: u8 = 1;
pub const SECTION: u8 = 2;

const FADE_FILTER: &CStr = c"color_filter_v2";

static TEXT_SOURCE: OnceLock<&'static CStr> = OnceLock::new();

pub fn kind(k: u8) -> Kind {
    match k {
        SONG_INFO => Kind::SongInfo,
        SECTION => Kind::Section,
        _ => Kind::Lyrics,
    }
}

pub unsafe fn exists(id: &CStr) -> bool {
    !obs_source_get_display_name(id.as_ptr()).is_null()
}

pub fn text_source() -> &'static CStr {
    if let Some(id) = TEXT_SOURCE.get() {
        return id;
    }
    let candidates: &[&'static CStr] = if cfg!(windows) {
        &[c"text_gdiplus_v3", c"text_gdiplus_v2", c"text_gdiplus"]
    } else {
        &[c"text_ft2_source_v2", c"text_ft2_source"]
    };
    match candidates.iter().find(|id| unsafe { exists(id) }) {
        Some(id) => TEXT_SOURCE.get_or_init(|| id),
        None => candidates[0],
    }
}

pub struct Source {
    kind: Kind,
    me: *mut obs_source_t,
    child: *mut obs_source_t,
    fade: *mut obs_source_t,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    target: Option<String>,
    connection: Option<Arc<Connection>>,
    options: Options,
    fade_seconds: f32,
    fader: Fader,
}

impl State {
    unsafe fn read(&mut self, settings: *mut obs_data_t) {
        self.options = Options {
            blank_when_hidden: boolean(settings, c"blank_when_hidden"),
            uppercase: boolean(settings, c"uppercase"),
            max_lines: int(settings, c"max_lines").max(0) as usize,
            join_lines: boolean(settings, c"join_lines"),
            songs_only: boolean(settings, c"songs_only"),
            template: string(settings, c"template"),
            license: string(settings, c"license"),
        };
        self.fade_seconds = int(settings, c"fade_ms").clamp(0, 5000) as f32 / 1000.0;
        let target = string(settings, c"computer").trim().to_string();
        if self.target.as_ref() != Some(&target) {
            self.connection = Some(connection::connect(&target));
            self.target = Some(target);
        }
    }

    unsafe fn tick(&mut self, source: &Source, seconds: f32) {
        let Some(connection) = &self.connection else { return };
        let target = text::text(source.kind, connection.current().as_ref(), &self.options);
        let duration = if source.fade.is_null() { 0.0 } else { self.fade_seconds };
        let changed = self.fader.tick(&target, seconds, duration);
        if changed.text {
            data::set_text(source.child, &self.fader.shown);
        }
        if changed.opacity {
            data::set_opacity(source.fade, self.fader.opacity);
        }
    }
}

impl Source {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> Status {
        self.state().connection.as_ref().map(|c| c.status()).unwrap_or_default()
    }

    pub fn child(&self) -> *mut obs_source_t {
        self.child
    }
}

pub unsafe fn source<'a>(data: *mut c_void) -> &'a Source {
    &*(data as *const Source)
}

pub unsafe extern "C" fn name<const K: u8>(_: *mut c_void) -> *const c_char {
    match kind(K) {
        Kind::Lyrics => c"PALTO Lyrics".as_ptr(),
        Kind::SongInfo => c"PALTO Song Info".as_ptr(),
        Kind::Section => c"PALTO Section".as_ptr(),
    }
}

pub unsafe extern "C" fn create<const K: u8>(settings: *mut obs_data_t, me: *mut obs_source_t) -> *mut c_void {
    let style = data::style(settings);
    obs_data_set_string(style, c"text".as_ptr(), c"".as_ptr());
    let child = obs_source_create_private(text_source().as_ptr(), c"PALTO text".as_ptr(), style);
    obs_data_release(style);
    let fade = if exists(FADE_FILTER) { obs_source_create_private(FADE_FILTER.as_ptr(), c"PALTO fade".as_ptr(), std::ptr::null_mut()) } else { std::ptr::null_mut() };
    if !child.is_null() {
        obs_source_add_active_child(me, child);
        if !fade.is_null() {
            obs_source_filter_add(child, fade);
        }
    }
    let mut state = State::default();
    state.read(settings);
    Box::into_raw(Box::new(Source { kind: kind(K), me, child, fade, state: Mutex::new(state) })) as *mut c_void
}

pub unsafe extern "C" fn destroy(data: *mut c_void) {
    let source = Box::from_raw(data as *mut Source);
    if !source.child.is_null() {
        obs_source_remove_active_child(source.me, source.child);
        if !source.fade.is_null() {
            obs_source_filter_remove(source.child, source.fade);
        }
        obs_source_release(source.child);
    }
    if !source.fade.is_null() {
        obs_source_release(source.fade);
    }
}

pub unsafe extern "C" fn update(data: *mut c_void, settings: *mut obs_data_t) {
    let source = source(data);
    let mut state = source.state();
    state.read(settings);
    if !source.child.is_null() {
        let style = data::style(settings);
        obs_source_update(source.child, style);
        obs_data_release(style);
        data::set_text(source.child, &state.fader.shown);
    }
}

pub unsafe extern "C" fn video_tick(data: *mut c_void, seconds: f32) {
    let source = source(data);
    source.state().tick(source, seconds);
}

pub unsafe extern "C" fn video_render(data: *mut c_void, _: *mut gs_effect_t) {
    let source = source(data);
    if !source.child.is_null() {
        obs_source_video_render(source.child);
    }
}

pub unsafe extern "C" fn width(data: *mut c_void) -> u32 {
    let source = source(data);
    if source.child.is_null() { 0 } else { obs_source_get_width(source.child) }
}

pub unsafe extern "C" fn height(data: *mut c_void) -> u32 {
    let source = source(data);
    if source.child.is_null() { 0 } else { obs_source_get_height(source.child) }
}

pub unsafe extern "C" fn children(data: *mut c_void, callback: EnumProc, param: *mut c_void) {
    let source = source(data);
    if !source.child.is_null() {
        callback(source.me, source.child, param);
    }
}
