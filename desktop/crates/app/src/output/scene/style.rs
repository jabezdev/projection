
use protocol::{
    OutputConfig, TextAlign as ConfigAlign, VerticalAlign,
};
use render_engine::{
    css_color, ConfidenceStyle, Direction, Easing, LyricStyle, Shadow, ShaderSpec, Stroke, TextAlign, Transition, TransitionKind, Vertical, VideoKey,
};


pub fn lyric_style(config: &OutputConfig) -> LyricStyle {
    let mut box_color = css_color(&config.box_color, [0.0, 0.0, 0.0, 1.0]);
    box_color[3] *= (config.box_opacity as f32 / 100.0).clamp(0.0, 1.0);
    LyricStyle {
        font: config.font.clone(),
        size: config.size as f32,
        weight: config.weight.clamp(1.0, 1000.0) as u16,
        italic: config.italic,
        caps: config.caps,
        color: css_color(&config.color, [1.0, 1.0, 1.0, 1.0]),
        align: match config.align {
            ConfigAlign::Left => TextAlign::Left,
            ConfigAlign::Center => TextAlign::Center,
            ConfigAlign::Right => TextAlign::Right,
        },
        vertical: match config.vertical {
            VerticalAlign::Top => Vertical::Top,
            VerticalAlign::Center => Vertical::Center,
            VerticalAlign::Bottom => Vertical::Bottom,
        },
        margin_x: config.margin_x as f32,
        margin_y: config.margin_y as f32,
        line_height: config.line_height as f32,
        letter_spacing: config.letter_spacing as f32,
        border: config.border as f32,
        border_color: css_color(&config.border_color, [1.0, 1.0, 1.0, 1.0]),
        box_color,
        shadow: config.text_shadow.then(|| Shadow {
            color: opacity(css_color(&config.shadow_color, [0.0, 0.0, 0.0, 1.0]), config.shadow_opacity),
            x: config.shadow_offset_x as f32,
            y: config.shadow_offset_y as f32,
            blur: config.shadow_size as f32,
        }),
        outline: (config.text_outline && config.outline_width > 0.0)
            .then(|| Stroke { color: css_color(&config.outline_color, [0.0, 0.0, 0.0, 1.0]), size: config.outline_width as f32 }),
        glow: (config.text_glow && config.glow_size > 0.0)
            .then(|| Stroke { color: opacity(css_color(&config.glow_color, [1.0; 4]), config.glow_opacity), size: config.glow_size as f32 }),
    }
}

/// `color` with a 0–100 opacity applied.
pub(super) fn opacity(mut color: render_engine::Rgba, percent: f64) -> render_engine::Rgba {
    color[3] *= (percent as f32 / 100.0).clamp(0.0, 1.0);
    color
}

/// The shader background named by a `shader:<preset>` path, in the
/// configured colors and speed.
pub fn shader_key(config: &OutputConfig, path: &str) -> VideoKey {
    let preset = path.trim_start_matches(protocol::SHADER_PREFIX);
    VideoKey::Shader(ShaderSpec::encode(preset, &config.shader_color_a, &config.shader_color_b, config.shader_speed as f32, config.shader_animate))
}

pub fn transition(config: &OutputConfig) -> Transition {
    use protocol::{Easing as E, TransitionDirection as D, TransitionKind as K};
    Transition {
        kind: match config.transition_kind {
            K::Fade => TransitionKind::Fade,
            K::Cut => TransitionKind::Cut,
            K::Slide => TransitionKind::Slide,
        },
        easing: match config.transition_easing {
            E::Linear => Easing::Linear,
            E::Ease => Easing::Ease,
            E::EaseIn => Easing::EaseIn,
            E::EaseOut => Easing::EaseOut,
            E::EaseInOut => Easing::EaseInOut,
        },
        direction: match config.transition_direction {
            D::Left => Direction::Left,
            D::Right => Direction::Right,
            D::Up => Direction::Up,
            D::Down => Direction::Down,
        },
    }
}

pub fn confidence_style(config: &OutputConfig) -> ConfidenceStyle {
    ConfidenceStyle {
        font: config.confidence_font.clone(),
        line_height: config.confidence_line_height as f32,
        caps: config.confidence_caps,
        clock: config.confidence_clock,
        tape: config.confidence_tape,
    }
}
