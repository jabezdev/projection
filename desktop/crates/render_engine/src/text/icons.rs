use std::collections::HashMap;
use std::sync::OnceLock;



/// Lucide (https://lucide.dev, ISC license) as a font, and its icon names
/// with their code points; see `assets/lucide/`.
pub(super) const ICON_FONT: &[u8] = include_bytes!("../../assets/lucide/lucide.ttf");
pub(super) const ICON_FAMILY: &str = "lucide";
pub(super) const ICON_NAMES: &str = include_str!("../../assets/lucide/names.txt");

pub(super) fn icon_char(name: &str) -> Option<char> {
    static ICONS: OnceLock<HashMap<&'static str, char>> = OnceLock::new();
    let icons = ICONS.get_or_init(|| {
        ICON_NAMES
            .lines()
            .filter_map(|line| line.split_once(' '))
            .filter_map(|(name, code)| Some((name, char::from_u32(u32::from_str_radix(code, 16).ok()?)?)))
            .collect()
    });
    icons.get(name).copied()
}

/// Replaces `:name:` icon codes with the icon's character and returns the
/// byte ranges of those characters, to be shaped with the icon font.
pub fn expand_icons(text: &str) -> (String, Vec<std::ops::Range<usize>>) {
    if !text.contains(':') {
        return (text.to_string(), Vec::new());
    }
    let mut out = String::with_capacity(text.len());
    let mut icons = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let name_len = after.find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')).unwrap_or(after.len());
        let icon = (name_len > 0 && after[name_len..].starts_with(':')).then(|| icon_char(&after[..name_len])).flatten();
        match icon {
            Some(icon) => {
                icons.push(out.len()..out.len() + icon.len_utf8());
                out.push(icon);
                rest = &after[name_len + 1..];
            }
            None => {
                out.push(':');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    (out, icons)
}
