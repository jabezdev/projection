//! Output styling saved in `output-settings.json`. Field names and defaults
//! match the operator's `defaultOutputConfig` so existing files load unchanged.

use serde::{Deserialize, Serialize};

pub const OUTPUT_CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlign {
    Top,
    Center,
    #[default]
    Bottom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundFit {
    #[default]
    Cover,
    Contain,
}

/// Sizes are in the operator's units: `size`, `marginX` and `letterSpacing / 16`
/// are percentages of output width; `marginY * 9/16` likewise; `lineHeight`
/// is a multiple of the font size; `transition` is milliseconds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct OutputConfig {
    pub version: u32,
    pub font: String,
    pub size: f64,
    pub weight: f64,
    pub italic: bool,
    pub caps: bool,
    pub color: String,
    pub align: TextAlign,
    pub vertical: VerticalAlign,
    pub margin_x: f64,
    pub margin_y: f64,
    pub line_height: f64,
    pub letter_spacing: f64,
    // Outline, shadow and glow are preserved but not rendered natively yet.
    pub outline: f64,
    pub outline_color: String,
    pub shadow: f64,
    pub shadow_x: f64,
    pub shadow_y: f64,
    pub shadow_color: String,
    pub glow: f64,
    pub glow_color: String,
    pub border: f64,
    pub border_color: String,
    pub box_color: String,
    /// Percent, 0–100.
    pub box_opacity: f64,
    pub background_fit: BackgroundFit,
    pub transition: f64,
    pub confidence_size: f64,
    pub logo_path: String,
    /// Fields written by newer versions survive a round trip through this one.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            version: OUTPUT_CONFIG_VERSION,
            font: "Arial".into(),
            size: 6.0,
            weight: 800.0,
            italic: false,
            caps: false,
            color: "#ffffff".into(),
            align: TextAlign::Center,
            vertical: VerticalAlign::Bottom,
            margin_x: 5.0,
            margin_y: 10.0,
            line_height: 1.2,
            letter_spacing: 0.0,
            outline: 0.0,
            outline_color: "#000000".into(),
            shadow: 16.0,
            shadow_x: 0.0,
            shadow_y: 3.0,
            shadow_color: "#000000".into(),
            glow: 0.0,
            glow_color: "#ffffff".into(),
            border: 0.0,
            border_color: "#ffffff".into(),
            box_color: "#000000".into(),
            box_opacity: 0.0,
            background_fit: BackgroundFit::Cover,
            transition: 250.0,
            confidence_size: 4.0,
            logo_path: String::new(),
            extra: serde_json::Map::new(),
        }
    }
}

impl OutputConfig {
    /// Rejects values no output can render, with a message for the operator.
    pub fn validate(&self) -> Result<(), String> {
        let finite = [
            ("size", self.size), ("weight", self.weight), ("marginX", self.margin_x), ("marginY", self.margin_y),
            ("lineHeight", self.line_height), ("letterSpacing", self.letter_spacing), ("border", self.border),
            ("boxOpacity", self.box_opacity), ("transition", self.transition), ("confidenceSize", self.confidence_size),
        ];
        for (name, value) in finite {
            if !value.is_finite() {
                return Err(format!("Output setting {name} must be a number"));
            }
        }
        if self.size <= 0.0 || self.confidence_size <= 0.0 || self.line_height <= 0.0 {
            return Err("Text size and line height must be greater than zero".into());
        }
        if !(0.0..=100.0).contains(&self.box_opacity) {
            return Err("Box opacity must be between 0 and 100".into());
        }
        if self.transition < 0.0 || self.border < 0.0 {
            return Err("Transition and border cannot be negative".into());
        }
        if self.font.trim().is_empty() {
            return Err("Choose a font".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings file saved by the webview-era operator UI.
    const SAVED: &str = r##"{
      "align": "center", "backgroundFit": "cover", "border": 2, "borderColor": "#000000",
      "boxColor": "#000000", "boxOpacity": 100, "caps": true, "color": "#ffffff",
      "confidenceSize": 3, "font": "Arial", "glow": 0, "glowColor": "#ffffff", "italic": false,
      "letterSpacing": 0, "lineHeight": 0.9, "logoPath": "", "marginX": 5, "marginY": 25,
      "outline": 0, "outlineColor": "#000000", "shadow": 16, "shadowColor": "#000000",
      "shadowX": 0, "shadowY": 3, "size": 3.8, "transition": 100, "vertical": "top", "weight": 800
    }"##;

    #[test]
    fn existing_settings_files_load_with_every_field() {
        let config: OutputConfig = serde_json::from_str(SAVED).unwrap();
        assert_eq!(config.version, OUTPUT_CONFIG_VERSION);
        assert_eq!((config.size, config.line_height, config.margin_y), (3.8, 0.9, 25.0));
        assert_eq!((config.vertical, config.border, config.box_opacity), (VerticalAlign::Top, 2.0, 100.0));
        assert!(config.caps);
        assert!(config.extra.is_empty(), "every saved field is typed");
        config.validate().unwrap();
    }

    #[test]
    fn saving_keeps_the_operator_field_names() {
        let saved: serde_json::Value = serde_json::from_str(SAVED).unwrap();
        let config: OutputConfig = serde_json::from_value(saved.clone()).unwrap();
        let mut written = serde_json::to_value(&config).unwrap();
        written.as_object_mut().unwrap().remove("version");
        let (written, saved) = (written.as_object().unwrap(), saved.as_object().unwrap());
        assert_eq!(written.keys().collect::<Vec<_>>(), saved.keys().collect::<Vec<_>>());
        for (key, value) in saved {
            // JSON numbers compare by value: `0.9` must not become `0.8999999761`.
            match value.as_f64() {
                Some(number) => assert_eq!(written[key].as_f64(), Some(number), "{key}"),
                None => assert_eq!(&written[key], value, "{key}"),
            }
        }
    }

    #[test]
    fn missing_fields_take_operator_defaults_and_unknown_fields_survive() {
        let config: OutputConfig = serde_json::from_str(r#"{"size": 5, "futureOption": {"a": 1}}"#).unwrap();
        assert_eq!(config.size, 5.0);
        assert_eq!(config.font, "Arial");
        assert_eq!(config.transition, 250.0);
        let written = serde_json::to_value(&config).unwrap();
        assert_eq!(written["futureOption"], serde_json::json!({"a": 1}));
    }

    #[test]
    fn invalid_values_are_rejected() {
        let config = OutputConfig { box_opacity: 150.0, ..OutputConfig::default() };
        assert!(config.validate().is_err());
        assert!(serde_json::from_str::<OutputConfig>(r#"{"align": "diagonal"}"#).is_err());
    }
}
