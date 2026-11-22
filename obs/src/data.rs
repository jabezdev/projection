use std::ffi::{c_char, c_int, CStr, CString};

use crate::ffi::*;

pub fn cstring(text: &str) -> CString {
    CString::new(text.replace('\0', "")).unwrap_or_default()
}

pub fn log(level: c_int, message: &str) {
    let message = cstring(&format!("[palto-obs] {message}"));
    unsafe { blog(level, c"%s".as_ptr(), message.as_ptr()) };
}

pub unsafe fn string(data: *mut obs_data_t, name: &CStr) -> String {
    let value = obs_data_get_string(data, name.as_ptr());
    if value.is_null() {
        return String::new();
    }
    CStr::from_ptr(value).to_string_lossy().into_owned()
}

pub unsafe fn int(data: *mut obs_data_t, name: &CStr) -> i64 {
    obs_data_get_int(data, name.as_ptr())
}

pub unsafe fn boolean(data: *mut obs_data_t, name: &CStr) -> bool {
    obs_data_get_bool(data, name.as_ptr())
}

pub unsafe fn update(source: *mut obs_source_t, change: impl FnOnce(*mut obs_data_t)) {
    if source.is_null() {
        return;
    }
    let data = obs_data_create();
    change(data);
    obs_source_update(source, data);
    obs_data_release(data);
}

pub unsafe fn set_text(source: *mut obs_source_t, text: &str) {
    let text = cstring(text);
    update(source, |data| obs_data_set_string(data, c"text".as_ptr(), text.as_ptr()));
}

pub unsafe fn set_opacity(filter: *mut obs_source_t, opacity: f32) {
    update(filter, |data| obs_data_set_double(data, c"opacity".as_ptr(), opacity as f64));
}

pub const FILE_SETTINGS: [&CStr; 9] = [c"text", c"from_file", c"text_file", c"use_file", c"file", c"log_mode", c"log_lines", c"chatlog", c"chatlog_lines"];

pub unsafe fn style(settings: *mut obs_data_t) -> *mut obs_data_t {
    let style = obs_data_get_defaults(settings);
    obs_data_apply(style, settings);
    style
}

unsafe fn skipped(name: *const c_char, skip: &[&CStr]) -> bool {
    name.is_null() || skip.iter().any(|s| CStr::from_ptr(name) == *s)
}

pub unsafe fn copy_default_values(from: *mut obs_data_t, to: *mut obs_data_t, skip: &[&CStr]) {
    let mut item = obs_data_first(from);
    while !item.is_null() {
        let name = obs_data_item_get_name(item);
        if !skipped(name, skip) && obs_data_item_has_default_value(item) {
            match obs_data_item_gettype(item) {
                OBS_DATA_STRING => obs_data_set_default_string(to, name, obs_data_item_get_default_string(item)),
                OBS_DATA_NUMBER if obs_data_item_numtype(item) == OBS_DATA_NUM_INT => obs_data_set_default_int(to, name, obs_data_item_get_default_int(item)),
                OBS_DATA_NUMBER => obs_data_set_default_double(to, name, obs_data_item_get_default_double(item)),
                OBS_DATA_BOOLEAN => obs_data_set_default_bool(to, name, obs_data_item_get_default_bool(item)),
                OBS_DATA_OBJECT => {
                    let object = obs_data_item_get_default_obj(item);
                    if !object.is_null() {
                        let copy = obs_data_create();
                        copy_default_values(object, copy, &[]);
                        obs_data_apply(copy, object);
                        obs_data_set_default_obj(to, name, copy);
                        obs_data_release(copy);
                        obs_data_release(object);
                    }
                }
                _ => {}
            }
        }
        obs_data_item_next(&mut item);
    }
}

pub unsafe fn copy_defaults(id: &CStr, to: *mut obs_data_t) {
    let defaults = obs_get_source_defaults(id.as_ptr());
    if !defaults.is_null() {
        copy_default_values(defaults, to, &FILE_SETTINGS);
        obs_data_release(defaults);
    }
}

pub unsafe fn set_default_font_size(to: *mut obs_data_t, size: i64) {
    let font = obs_data_get_default_obj(to, c"font".as_ptr());
    if !font.is_null() {
        obs_data_set_default_int(font, c"size".as_ptr(), size);
        obs_data_release(font);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn font_size(data: *mut obs_data_t) -> (String, i64) {
        let font = obs_data_get_default_obj(data, c"font".as_ptr());
        let result = (string(font, c"face"), int(font, c"size"));
        obs_data_release(font);
        result
    }

    #[test]
    fn defaults_copy_with_their_objects() {
        unsafe {
            let from = obs_data_create();
            let font = obs_data_create();
            obs_data_set_default_string(font, c"face".as_ptr(), c"Sans Serif".as_ptr());
            obs_data_set_default_int(font, c"size".as_ptr(), 256);
            obs_data_set_default_obj(from, c"font".as_ptr(), font);
            obs_data_release(font);
            obs_data_set_default_int(from, c"color1".as_ptr(), 0xFFFFFFFF);
            obs_data_set_default_bool(from, c"outline".as_ptr(), false);
            obs_data_set_default_string(from, c"text".as_ptr(), c"skip me".as_ptr());
            let to = obs_data_create();
            copy_default_values(from, to, &FILE_SETTINGS);
            obs_data_release(from);
            set_default_font_size(to, 64);
            assert_eq!(font_size(to), ("Sans Serif".to_string(), 64));
            assert_eq!(int(to, c"color1"), 0xFFFFFFFF);
            assert_eq!(string(to, c"text"), "");
            obs_data_release(to);
        }
    }
}
