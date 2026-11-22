use std::ffi::{c_void, CStr};
use std::time::Duration;

use crate::data::{self, cstring};
use crate::discover;
use crate::ffi::*;
use crate::source::{self, kind, text_source};
use crate::text::{Kind, DEFAULT_TEMPLATE};

const SCAN_LIMIT: Duration = Duration::from_secs(4);
const PLACEHOLDERS: &CStr = c"Use {title}, {authors}, {words}, {music}, {translation}, {copyright}, {ccli}, {publisher}, {year}, {license} and {section}. Parts between · are left out when they are empty, and so are empty lines.";

pub unsafe extern "C" fn defaults<const K: u8>(settings: *mut obs_data_t) {
    data::copy_defaults(text_source(), settings);
    let size = match kind(K) {
        Kind::Lyrics => 64,
        Kind::SongInfo => 28,
        Kind::Section => 32,
    };
    data::set_default_font_size(settings, size);
    obs_data_set_default_string(settings, c"computer".as_ptr(), c"".as_ptr());
    obs_data_set_default_bool(settings, c"blank_when_hidden".as_ptr(), true);
    obs_data_set_default_int(settings, c"fade_ms".as_ptr(), 300);
    obs_data_set_default_bool(settings, c"uppercase".as_ptr(), false);
    obs_data_set_default_int(settings, c"max_lines".as_ptr(), 0);
    obs_data_set_default_bool(settings, c"join_lines".as_ptr(), false);
    obs_data_set_default_bool(settings, c"songs_only".as_ptr(), false);
    let template = cstring(DEFAULT_TEMPLATE);
    obs_data_set_default_string(settings, c"template".as_ptr(), template.as_ptr());
    obs_data_set_default_string(settings, c"license".as_ptr(), c"".as_ptr());
}

unsafe fn fill_computers(list: *mut obs_property_t, found: &[discover::Found]) {
    obs_property_list_clear(list);
    obs_property_list_add_string(list, c"First PALTO found".as_ptr(), c"".as_ptr());
    for computer in found {
        let label = cstring(&format!("{} ({})", computer.name, computer.addr.ip()));
        let value = cstring(&computer.name);
        obs_property_list_add_string(list, label.as_ptr(), value.as_ptr());
    }
}

unsafe extern "C" fn scan(props: *mut obs_properties_t, _: *mut obs_property_t, _: *mut c_void) -> bool {
    let found = discover::scan_blocking(SCAN_LIMIT);
    fill_computers(obs_properties_get(props, c"computer".as_ptr()), &found);
    true
}

unsafe fn connection_group(props: *mut obs_properties_t, data: *mut c_void) {
    let group = obs_properties_create();
    let computer = obs_properties_add_list(group, c"computer".as_ptr(), c"PALTO computer".as_ptr(), OBS_COMBO_TYPE_EDITABLE, OBS_COMBO_FORMAT_STRING);
    fill_computers(computer, &discover::last_found());
    obs_property_set_long_description(computer, c"Pick a computer, or type its name or address, for example 192.168.1.20.".as_ptr());
    obs_properties_add_button2(group, c"scan".as_ptr(), c"Scan the network again".as_ptr(), scan, std::ptr::null_mut());
    if !data.is_null() {
        let status = source::source(data).status();
        let text = cstring(&status.describe());
        let info = obs_properties_add_text(group, c"status".as_ptr(), text.as_ptr(), OBS_TEXT_INFO);
        if status.is_problem() {
            obs_property_text_set_info_type(info, OBS_TEXT_INFO_WARNING);
        }
    }
    obs_properties_add_group(props, c"palto".as_ptr(), c"PALTO".as_ptr(), OBS_GROUP_NORMAL, group);
}

unsafe fn display_group(props: *mut obs_properties_t, kind: Kind) {
    let group = obs_properties_create();
    let hide = obs_properties_add_bool(group, c"blank_when_hidden".as_ptr(), c"Hide in OBS when the lyrics are hidden in PALTO".as_ptr());
    obs_property_set_long_description(hide, c"Follows PALTO's Clear, Black and Logo. Turn this off to keep the text in OBS while the projection screen hides it.".as_ptr());
    let fade = obs_properties_add_int_slider(group, c"fade_ms".as_ptr(), c"Fade".as_ptr(), 0, 2000, 50);
    obs_property_int_set_suffix(fade, c" ms".as_ptr());
    obs_properties_add_bool(group, c"uppercase".as_ptr(), c"Capital letters".as_ptr());
    match kind {
        Kind::Lyrics => {
            let lines = obs_properties_add_int(group, c"max_lines".as_ptr(), c"Most lines (0 shows all)".as_ptr(), 0, 20, 1);
            obs_property_set_long_description(lines, c"Shows only the first lines of each slide.".as_ptr());
            obs_properties_add_bool(group, c"join_lines".as_ptr(), c"Put the lines of a slide on one line".as_ptr());
            obs_properties_add_bool(group, c"songs_only".as_ptr(), c"Show songs only (hide other text slides)".as_ptr());
        }
        Kind::SongInfo => {
            let template = obs_properties_add_text(group, c"template".as_ptr(), c"Credit text".as_ptr(), OBS_TEXT_MULTILINE);
            obs_property_set_long_description(template, PLACEHOLDERS.as_ptr());
            let license = obs_properties_add_text(group, c"license".as_ptr(), c"CCLI licence number".as_ptr(), OBS_TEXT_DEFAULT);
            obs_property_set_long_description(license, c"Your church's streaming licence number, shown by {license}.".as_ptr());
        }
        Kind::Section => {}
    }
    obs_properties_add_group(props, c"display".as_ptr(), c"What to show".as_ptr(), OBS_GROUP_NORMAL, group);
}

unsafe fn style_group(props: *mut obs_properties_t, data: *mut c_void) {
    if data.is_null() {
        return;
    }
    let child = source::source(data).child();
    if child.is_null() {
        return;
    }
    let style = obs_source_properties(child);
    if style.is_null() {
        return;
    }
    for name in data::FILE_SETTINGS {
        obs_properties_remove_by_name(style, name.as_ptr());
    }
    obs_properties_add_group(props, c"style".as_ptr(), c"Text style".as_ptr(), OBS_GROUP_NORMAL, style);
}

pub unsafe extern "C" fn properties<const K: u8>(data: *mut c_void) -> *mut obs_properties_t {
    let props = obs_properties_create();
    connection_group(props, data);
    display_group(props, kind(K));
    style_group(props, data);
    props
}
