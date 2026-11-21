use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DisplayName {
    pub friendly: String,
    pub connector: Option<&'static str>,
    pub device: Option<String>,
}

#[cfg(windows)]
pub fn display_names() -> HashMap<String, DisplayName> {
    match win::query() {
        Ok(names) => names,
        Err(code) => {
            log::warn!("Reading display names failed (error {code}); showing system names");
            HashMap::new()
        }
    }
}

#[cfg(not(windows))]
pub fn display_names() -> HashMap<String, DisplayName> {
    HashMap::new()
}

#[cfg(windows)]
pub fn refresh_rate(system_name: &str) -> Option<u32> {
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
    let name: Vec<u16> = system_name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `mode` is a zeroed DEVMODEW with its size set, as the API requires.
    unsafe {
        let mut mode: DEVMODEW = std::mem::zeroed();
        mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        (EnumDisplaySettingsW(name.as_ptr(), ENUM_CURRENT_SETTINGS, &mut mode) != 0 && mode.dmDisplayFrequency > 1).then_some(mode.dmDisplayFrequency)
    }
}

#[cfg(not(windows))]
pub fn refresh_rate(_system_name: &str) -> Option<u32> {
    None
}

pub fn system_number(system_name: &str) -> Option<u32> {
    let digits: String = system_name.chars().rev().take_while(char::is_ascii_digit).collect::<Vec<_>>().into_iter().rev().collect();
    system_name.to_ascii_uppercase().contains("DISPLAY").then(|| digits.parse().ok()).flatten()
}

pub fn readable_name(system_name: Option<&str>, info: Option<&DisplayName>) -> String {
    if let Some(info) = info {
        if !info.friendly.trim().is_empty() {
            return info.friendly.trim().to_string();
        }
        if info.connector == Some("Built-in") {
            return "Built-in display".into();
        }
    }
    match system_name {
        Some(name) => match system_number(name) {
            Some(number) => format!("Display {number}"),
            None if name.trim().is_empty() => "Display".into(),
            None => name.trim_start_matches(r"\\.\").to_string(),
        },
        None => "Display".into(),
    }
}

pub fn aspect_label(width: u32, height: u32) -> String {
    if width == 0 || height == 0 {
        return String::new();
    }
    const COMMON: [(&str, f64); 9] = [
        ("16:9", 16.0 / 9.0),
        ("16:10", 16.0 / 10.0),
        ("4:3", 4.0 / 3.0),
        ("5:4", 5.0 / 4.0),
        ("3:2", 3.0 / 2.0),
        ("21:9", 64.0 / 27.0),
        ("32:9", 32.0 / 9.0),
        ("9:16", 9.0 / 16.0),
        ("3:4", 3.0 / 4.0),
    ];
    let ratio = width as f64 / height as f64;
    let tolerance = |name: &str| if name == "21:9" { 0.03 } else { 0.012 };
    if let Some((name, _)) = COMMON.iter().find(|(name, value)| (ratio / value - 1.0).abs() <= tolerance(name)) {
        return (*name).to_string();
    }
    format!("{ratio:.2}:1")
}

#[cfg(windows)]
mod win {
    use std::collections::HashMap;

    use windows_sys::Win32::Devices::Display::{
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EXTERNAL,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DVI, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HD15, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_MIRACAST, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
        DISPLAYCONFIG_TARGET_DEVICE_NAME, DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY, QDC_ONLY_ACTIVE_PATHS,
    };
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};

    use super::DisplayName;

    fn text(wide: &[u16]) -> String {
        let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
        String::from_utf16_lossy(&wide[..end])
    }

    fn connector(technology: DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY) -> Option<&'static str> {
        Some(match technology {
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI => "HDMI",
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EXTERNAL => "DisplayPort",
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DVI => "DVI",
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HD15 => "VGA",
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_MIRACAST => "Wireless",
            DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED => "Built-in",
            _ => return None,
        })
    }

    pub fn query() -> Result<HashMap<String, DisplayName>, u32> {
        let mut paths = Vec::new();
        for _ in 0..3 {
            let (mut path_count, mut mode_count) = (0u32, 0u32);
            let status = unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count) };
            if status != ERROR_SUCCESS {
                return Err(status);
            }
            paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
            let mut modes = vec![unsafe { std::mem::zeroed::<DISPLAYCONFIG_MODE_INFO>() }; mode_count as usize];
            let status = unsafe { QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut path_count, paths.as_mut_ptr(), &mut mode_count, modes.as_mut_ptr(), std::ptr::null_mut()) };
            if status == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            if status != ERROR_SUCCESS {
                return Err(status);
            }
            paths.truncate(path_count as usize);
            break;
        }
        let mut names: HashMap<String, DisplayName> = HashMap::new();
        for path in &paths {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: path.sourceInfo.adapterId,
                    id: path.sourceInfo.id,
                },
                ..Default::default()
            };
            if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != 0 {
                continue;
            }
            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                    adapterId: path.targetInfo.adapterId,
                    id: path.targetInfo.id,
                },
                ..Default::default()
            };
            if unsafe { DisplayConfigGetDeviceInfo(&mut target.header) } != 0 {
                continue;
            }
            let found = DisplayName {
                friendly: text(&target.monitorFriendlyDeviceName),
                connector: connector(target.outputTechnology),
                device: Some(text(&target.monitorDevicePath)).filter(|path| !path.is_empty()),
            };
            names
                .entry(text(&source.viewGdiDeviceName))
                .and_modify(|existing| {
                    if !found.friendly.is_empty() && existing.friendly != found.friendly {
                        existing.friendly = if existing.friendly.is_empty() { found.friendly.clone() } else { format!("{} + {}", existing.friendly, found.friendly) };
                    }
                })
                .or_insert(found);
        }
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_common_shapes() {
        assert_eq!(aspect_label(1920, 1080), "16:9");
        assert_eq!(aspect_label(1366, 768), "16:9");
        assert_eq!(aspect_label(1920, 1200), "16:10");
        assert_eq!(aspect_label(1024, 768), "4:3");
        assert_eq!(aspect_label(1280, 1024), "5:4");
        assert_eq!(aspect_label(3440, 1440), "21:9");
        assert_eq!(aspect_label(2560, 1080), "21:9");
        assert_eq!(aspect_label(1080, 1920), "9:16");
        assert_eq!(aspect_label(1000, 1000), "1.00:1");
        assert_eq!(aspect_label(0, 1080), "");
    }

    #[test]
    fn readable_names_prefer_the_monitor_model() {
        let benq = DisplayName { friendly: "BenQ GW2480".into(), connector: Some("HDMI"), device: None };
        assert_eq!(readable_name(Some(r"\\.\DISPLAY2"), Some(&benq)), "BenQ GW2480");
        let panel = DisplayName { connector: Some("Built-in"), ..Default::default() };
        assert_eq!(readable_name(Some(r"\\.\DISPLAY1"), Some(&panel)), "Built-in display");
        assert_eq!(readable_name(Some(r"\\.\DISPLAY3"), None), "Display 3");
        assert_eq!(readable_name(Some("HDMI-A-1"), None), "HDMI-A-1");
        assert_eq!(readable_name(None, None), "Display");
    }

    #[test]
    fn reads_windows_display_numbers() {
        assert_eq!(system_number(r"\\.\DISPLAY12"), Some(12));
        assert_eq!(system_number("HDMI-A-1"), None);
    }
}
