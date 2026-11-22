#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int, c_void};

#[repr(C)]
pub struct obs_source_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct obs_data_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct obs_data_item_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct obs_properties_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct obs_property_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct obs_module_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct gs_effect_t {
    _private: [u8; 0],
}

pub type EnumProc = unsafe extern "C" fn(parent: *mut obs_source_t, child: *mut obs_source_t, param: *mut c_void);
pub type Clicked = unsafe extern "C" fn(props: *mut obs_properties_t, property: *mut obs_property_t, data: *mut c_void) -> bool;

pub const OBS_SOURCE_TYPE_INPUT: c_int = 0;
pub const OBS_SOURCE_VIDEO: u32 = 1 << 0;
pub const OBS_SOURCE_CUSTOM_DRAW: u32 = 1 << 3;
pub const OBS_SOURCE_SRGB: u32 = 1 << 15;
pub const OBS_ICON_TYPE_TEXT: c_int = 10;

pub const OBS_DATA_STRING: c_int = 1;
pub const OBS_DATA_NUMBER: c_int = 2;
pub const OBS_DATA_BOOLEAN: c_int = 3;
pub const OBS_DATA_OBJECT: c_int = 4;
pub const OBS_DATA_NUM_INT: c_int = 1;

pub const OBS_TEXT_DEFAULT: c_int = 0;
pub const OBS_TEXT_MULTILINE: c_int = 2;
pub const OBS_TEXT_INFO: c_int = 3;
pub const OBS_TEXT_INFO_WARNING: c_int = 1;
pub const OBS_COMBO_TYPE_EDITABLE: c_int = 1;
pub const OBS_COMBO_FORMAT_STRING: c_int = 3;
pub const OBS_GROUP_NORMAL: c_int = 1;

pub const LOG_WARNING: c_int = 200;
pub const LOG_INFO: c_int = 300;

#[repr(C)]
pub struct obs_source_info {
    pub id: *const c_char,
    pub type_: c_int,
    pub output_flags: u32,
    pub get_name: Option<unsafe extern "C" fn(type_data: *mut c_void) -> *const c_char>,
    pub create: Option<unsafe extern "C" fn(settings: *mut obs_data_t, source: *mut obs_source_t) -> *mut c_void>,
    pub destroy: Option<unsafe extern "C" fn(data: *mut c_void)>,
    pub get_width: Option<unsafe extern "C" fn(data: *mut c_void) -> u32>,
    pub get_height: Option<unsafe extern "C" fn(data: *mut c_void) -> u32>,
    pub get_defaults: Option<unsafe extern "C" fn(settings: *mut obs_data_t)>,
    pub get_properties: Option<unsafe extern "C" fn(data: *mut c_void) -> *mut obs_properties_t>,
    pub update: Option<unsafe extern "C" fn(data: *mut c_void, settings: *mut obs_data_t)>,
    pub activate: Option<unsafe extern "C" fn(data: *mut c_void)>,
    pub deactivate: Option<unsafe extern "C" fn(data: *mut c_void)>,
    pub show: Option<unsafe extern "C" fn(data: *mut c_void)>,
    pub hide: Option<unsafe extern "C" fn(data: *mut c_void)>,
    pub video_tick: Option<unsafe extern "C" fn(data: *mut c_void, seconds: f32)>,
    pub video_render: Option<unsafe extern "C" fn(data: *mut c_void, effect: *mut gs_effect_t)>,
    pub filter_video: Option<unsafe extern "C" fn()>,
    pub filter_audio: Option<unsafe extern "C" fn()>,
    pub enum_active_sources: Option<unsafe extern "C" fn(data: *mut c_void, callback: EnumProc, param: *mut c_void)>,
    pub save: Option<unsafe extern "C" fn(data: *mut c_void, settings: *mut obs_data_t)>,
    pub load: Option<unsafe extern "C" fn(data: *mut c_void, settings: *mut obs_data_t)>,
    pub mouse_click: Option<unsafe extern "C" fn()>,
    pub mouse_move: Option<unsafe extern "C" fn()>,
    pub mouse_wheel: Option<unsafe extern "C" fn()>,
    pub focus: Option<unsafe extern "C" fn()>,
    pub key_click: Option<unsafe extern "C" fn()>,
    pub filter_remove: Option<unsafe extern "C" fn()>,
    pub type_data: *mut c_void,
    pub free_type_data: Option<unsafe extern "C" fn(type_data: *mut c_void)>,
    pub audio_render: Option<unsafe extern "C" fn()>,
    pub enum_all_sources: Option<unsafe extern "C" fn(data: *mut c_void, callback: EnumProc, param: *mut c_void)>,
    pub transition_start: Option<unsafe extern "C" fn()>,
    pub transition_stop: Option<unsafe extern "C" fn()>,
    pub get_defaults2: Option<unsafe extern "C" fn()>,
    pub get_properties2: Option<unsafe extern "C" fn()>,
    pub audio_mix: Option<unsafe extern "C" fn()>,
    pub icon_type: c_int,
}

#[cfg_attr(windows, link(name = "obs", kind = "raw-dylib"))]
#[cfg_attr(not(windows), link(name = "obs"))]
extern "C" {
    pub fn blog(level: c_int, format: *const c_char, message: *const c_char);
    pub fn obs_register_source_s(info: *const obs_source_info, size: usize);
    pub fn obs_source_get_display_name(id: *const c_char) -> *const c_char;
    pub fn obs_source_create_private(id: *const c_char, name: *const c_char, settings: *mut obs_data_t) -> *mut obs_source_t;
    pub fn obs_source_release(source: *mut obs_source_t);
    pub fn obs_source_update(source: *mut obs_source_t, settings: *mut obs_data_t);
    pub fn obs_source_video_render(source: *mut obs_source_t);
    pub fn obs_source_get_width(source: *mut obs_source_t) -> u32;
    pub fn obs_source_get_height(source: *mut obs_source_t) -> u32;
    pub fn obs_source_add_active_child(parent: *mut obs_source_t, child: *mut obs_source_t) -> bool;
    pub fn obs_source_remove_active_child(parent: *mut obs_source_t, child: *mut obs_source_t);
    pub fn obs_source_filter_add(source: *mut obs_source_t, filter: *mut obs_source_t);
    pub fn obs_source_filter_remove(source: *mut obs_source_t, filter: *mut obs_source_t);
    pub fn obs_source_properties(source: *const obs_source_t) -> *mut obs_properties_t;
    pub fn obs_get_source_defaults(id: *const c_char) -> *mut obs_data_t;

    pub fn obs_data_create() -> *mut obs_data_t;
    pub fn obs_data_release(data: *mut obs_data_t);
    pub fn obs_data_get_string(data: *mut obs_data_t, name: *const c_char) -> *const c_char;
    pub fn obs_data_get_int(data: *mut obs_data_t, name: *const c_char) -> i64;
    pub fn obs_data_get_bool(data: *mut obs_data_t, name: *const c_char) -> bool;
    pub fn obs_data_set_string(data: *mut obs_data_t, name: *const c_char, value: *const c_char);
    pub fn obs_data_set_double(data: *mut obs_data_t, name: *const c_char, value: f64);
    pub fn obs_data_apply(target: *mut obs_data_t, apply: *mut obs_data_t);
    pub fn obs_data_set_default_string(data: *mut obs_data_t, name: *const c_char, value: *const c_char);
    pub fn obs_data_set_default_int(data: *mut obs_data_t, name: *const c_char, value: i64);
    pub fn obs_data_set_default_double(data: *mut obs_data_t, name: *const c_char, value: f64);
    pub fn obs_data_set_default_bool(data: *mut obs_data_t, name: *const c_char, value: bool);
    pub fn obs_data_set_default_obj(data: *mut obs_data_t, name: *const c_char, value: *mut obs_data_t);
    pub fn obs_data_get_defaults(data: *mut obs_data_t) -> *mut obs_data_t;
    pub fn obs_data_get_default_obj(data: *mut obs_data_t, name: *const c_char) -> *mut obs_data_t;
    pub fn obs_data_first(data: *mut obs_data_t) -> *mut obs_data_item_t;
    pub fn obs_data_item_next(item: *mut *mut obs_data_item_t) -> bool;
    pub fn obs_data_item_get_name(item: *mut obs_data_item_t) -> *const c_char;
    pub fn obs_data_item_gettype(item: *mut obs_data_item_t) -> c_int;
    pub fn obs_data_item_numtype(item: *mut obs_data_item_t) -> c_int;
    pub fn obs_data_item_has_default_value(item: *mut obs_data_item_t) -> bool;
    pub fn obs_data_item_get_default_string(item: *mut obs_data_item_t) -> *const c_char;
    pub fn obs_data_item_get_default_int(item: *mut obs_data_item_t) -> i64;
    pub fn obs_data_item_get_default_double(item: *mut obs_data_item_t) -> f64;
    pub fn obs_data_item_get_default_bool(item: *mut obs_data_item_t) -> bool;
    pub fn obs_data_item_get_default_obj(item: *mut obs_data_item_t) -> *mut obs_data_t;

    pub fn obs_properties_create() -> *mut obs_properties_t;
    pub fn obs_properties_get(props: *mut obs_properties_t, name: *const c_char) -> *mut obs_property_t;
    pub fn obs_properties_remove_by_name(props: *mut obs_properties_t, name: *const c_char);
    pub fn obs_properties_add_bool(props: *mut obs_properties_t, name: *const c_char, description: *const c_char) -> *mut obs_property_t;
    pub fn obs_properties_add_int(props: *mut obs_properties_t, name: *const c_char, description: *const c_char, min: c_int, max: c_int, step: c_int) -> *mut obs_property_t;
    pub fn obs_properties_add_int_slider(props: *mut obs_properties_t, name: *const c_char, description: *const c_char, min: c_int, max: c_int, step: c_int) -> *mut obs_property_t;
    pub fn obs_properties_add_text(props: *mut obs_properties_t, name: *const c_char, description: *const c_char, kind: c_int) -> *mut obs_property_t;
    pub fn obs_properties_add_list(props: *mut obs_properties_t, name: *const c_char, description: *const c_char, kind: c_int, format: c_int) -> *mut obs_property_t;
    pub fn obs_properties_add_button2(props: *mut obs_properties_t, name: *const c_char, text: *const c_char, callback: Clicked, data: *mut c_void) -> *mut obs_property_t;
    pub fn obs_properties_add_group(props: *mut obs_properties_t, name: *const c_char, description: *const c_char, kind: c_int, group: *mut obs_properties_t) -> *mut obs_property_t;
    pub fn obs_property_list_add_string(property: *mut obs_property_t, name: *const c_char, value: *const c_char) -> usize;
    pub fn obs_property_list_clear(property: *mut obs_property_t);
    pub fn obs_property_set_long_description(property: *mut obs_property_t, text: *const c_char);
    pub fn obs_property_text_set_info_type(property: *mut obs_property_t, kind: c_int);
    pub fn obs_property_int_set_suffix(property: *mut obs_property_t, suffix: *const c_char);
}
