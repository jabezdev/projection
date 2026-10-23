//! Output styling saved in `output-settings.json`. Field names and defaults
//! match the operator's `defaultOutputConfig` so existing files load unchanged.

use serde::{Deserialize, Serialize};

pub const OUTPUT_CONFIG_VERSION: u32 = 2;

/// Built-in animated backgrounds, chosen as a `shader:<name>` background.
pub const SHADER_PRESETS: [&str; 3] = ["gradient", "aurora", "bokeh"];

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
pub enum TransitionKind {
    #[default]
    Fade,
    Cut,
    /// The new content pushes the old out in `TransitionDirection`.
    Slide,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    #[default]
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
}

/// Which way content moves during a slide.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransitionDirection {
    #[default]
    Left,
    Right,
    Up,
    Down,
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
/// is a multiple of the font size; `transition` is milliseconds. Border,
/// shadow, outline and glow sizes are sixteenths of a percent of the width.
///
/// The flat fields style the projection; `confidence*` fields style the
/// confidence screen. Older files' numeric `shadow`, `outline` and `glow`
/// (from the webview outputs) are not reused, so they cannot switch effects on.
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
    pub border: f64,
    pub border_color: String,
    pub box_color: String,
    /// Percent, 0–100.
    pub box_opacity: f64,
    pub background_fit: BackgroundFit,
    pub transition: f64,
    pub transition_kind: TransitionKind,
    pub transition_easing: Easing,
    pub transition_direction: TransitionDirection,
    pub text_shadow: bool,
    pub shadow_size: f64,
    pub shadow_offset_x: f64,
    pub shadow_offset_y: f64,
    pub shadow_color: String,
    /// Percent, 0–100.
    pub shadow_opacity: f64,
    pub text_outline: bool,
    pub outline_width: f64,
    pub outline_color: String,
    pub text_glow: bool,
    pub glow_size: f64,
    pub glow_color: String,
    /// Percent, 0–100.
    pub glow_opacity: f64,
    /// Colors and speed of the built-in shader backgrounds.
    pub shader_color_a: String,
    pub shader_color_b: String,
    pub shader_speed: f64,
    /// Off shows shader backgrounds as a still.
    pub shader_animate: bool,
    pub confidence_size: f64,
    pub confidence_font: String,
    pub confidence_line_height: f64,
    pub confidence_caps: bool,
    pub confidence_clock: bool,
    pub confidence_tape: bool,
    /// Fade of the confidence screen's video, in milliseconds.
    pub confidence_transition: f64,
    pub logo_path: String,
    /// Fields this version does not render survive a round trip: those written
    /// by newer versions, and the outline/shadow/glow of older ones.
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
            border: 0.0,
            border_color: "#ffffff".into(),
            box_color: "#000000".into(),
            box_opacity: 0.0,
            background_fit: BackgroundFit::Cover,
            transition: 250.0,
            transition_kind: TransitionKind::Fade,
            transition_easing: Easing::Linear,
            transition_direction: TransitionDirection::Left,
            text_shadow: false,
            shadow_size: 8.0,
            shadow_offset_x: 0.0,
            shadow_offset_y: 4.0,
            shadow_color: "#000000".into(),
            shadow_opacity: 80.0,
            text_outline: false,
            outline_width: 3.0,
            outline_color: "#000000".into(),
            text_glow: false,
            glow_size: 12.0,
            glow_color: "#ffffff".into(),
            glow_opacity: 60.0,
            shader_color_a: "#14213d".into(),
            shader_color_b: "#47c6bd".into(),
            shader_speed: 1.0,
            shader_animate: true,
            confidence_size: 4.0,
            confidence_font: "Segoe UI".into(),
            confidence_line_height: 1.35,
            confidence_caps: false,
            confidence_clock: true,
            confidence_tape: true,
            confidence_transition: 250.0,
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
            ("shadowSize", self.shadow_size), ("shadowOffsetX", self.shadow_offset_x), ("shadowOffsetY", self.shadow_offset_y),
            ("shadowOpacity", self.shadow_opacity), ("outlineWidth", self.outline_width), ("glowSize", self.glow_size),
            ("glowOpacity", self.glow_opacity), ("shaderSpeed", self.shader_speed), ("confidenceLineHeight", self.confidence_line_height),
            ("confidenceTransition", self.confidence_transition),
        ];
        for (name, value) in finite {
            if !value.is_finite() {
                return Err(format!("Output setting {name} must be a number"));
            }
        }
        if self.size <= 0.0 || self.confidence_size <= 0.0 || self.line_height <= 0.0 || self.confidence_line_height <= 0.0 {
            return Err("Text size and line height must be greater than zero".into());
        }
        if [self.box_opacity, self.shadow_opacity, self.glow_opacity].iter().any(|o| !(0.0..=100.0).contains(o)) {
            return Err("Opacity must be between 0 and 100".into());
        }
        if [self.transition, self.confidence_transition, self.border, self.shadow_size, self.outline_width, self.glow_size].iter().any(|v| *v < 0.0) {
            return Err("Transitions, borders and effect sizes cannot be negative".into());
        }
        if self.shader_speed <= 0.0 {
            return Err("Shader speed must be greater than zero".into());
        }
        if self.font.trim().is_empty() || self.confidence_font.trim().is_empty() {
            return Err("Choose a font".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings file saved by an earlier version, with outline, shadow and glow.
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
        let mut untyped: Vec<_> = config.extra.keys().map(String::as_str).collect();
        untyped.sort_unstable();
        assert_eq!(untyped, ["glow", "outline", "shadow", "shadowX", "shadowY"]);
        assert!(!config.text_shadow && !config.text_outline && !config.text_glow, "old effect sizes do not switch effects on");
        config.validate().unwrap();
    }

    #[test]
    fn saving_keeps_the_operator_field_names() {
        let saved: serde_json::Value = serde_json::from_str(SAVED).unwrap();
        let config: OutputConfig = serde_json::from_value(saved.clone()).unwrap();
        let mut written = serde_json::to_value(&config).unwrap();
        written.as_object_mut().unwrap().remove("version");
        let (written, saved) = (written.as_object().unwrap(), saved.as_object().unwrap());
        // Every saved key survives; fields added since are written too.
        assert!(saved.keys().all(|key| written.contains_key(key)));
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
