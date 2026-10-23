// Built-in animated backgrounds. Colors are sRGB-encoded values, as in the
// rest of the outputs; keep each preset calm and cheap (a few dozen ALU ops
// per pixel at 960x540).

struct Uniforms {
    color_a: vec4<f32>,
    color_b: vec4<f32>,
    resolution: vec2<f32>,
    time: f32,
    // 0 gradient, 1 aurora, 2 bokeh.
    preset: u32,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the target.
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let s = f * f * (3.0 - 2.0 * f);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

/// A slowly turning two-color gradient with a soft vignette.
fn gradient(uv: vec2<f32>, t: f32) -> vec3<f32> {
    let angle = t * 0.05;
    let dir = vec2<f32>(cos(angle), sin(angle));
    let p = (uv - 0.5) * vec2<f32>(u.resolution.x / u.resolution.y, 1.0);
    let wave = 0.06 * sin(p.y * 3.0 + t * 0.4) + 0.04 * noise(p * 2.0 + t * 0.05);
    let k = smoothstep(-0.6, 0.6, dot(p, dir) + wave);
    let vignette = 1.0 - 0.35 * dot(p, p);
    return mix(u.color_a.rgb, u.color_b.rgb, k) * vignette;
}

/// Soft flowing bands of the second color over a dark first color.
fn aurora(uv: vec2<f32>, t: f32) -> vec3<f32> {
    var color = u.color_a.rgb * (0.35 + 0.25 * uv.y);
    for (var i = 0; i < 3; i = i + 1) {
        let fi = f32(i);
        let center = 0.35 + 0.12 * fi + 0.08 * sin(uv.x * (2.0 + fi) + t * (0.25 + 0.07 * fi) + fi * 1.7)
            + 0.05 * (noise(vec2<f32>(uv.x * 3.0 + t * 0.1, fi)) - 0.5);
        let width = 0.05 + 0.03 * fi;
        let band = exp(-pow((uv.y - center) / width, 2.0));
        let tint = mix(u.color_b.rgb, mix(u.color_a.rgb, vec3<f32>(1.0), 0.5), fi * 0.3);
        color = color + tint * band * (0.55 - 0.12 * fi);
    }
    return min(color, vec3<f32>(1.0));
}

/// Out-of-focus lights drifting upward.
fn bokeh(uv: vec2<f32>, t: f32) -> vec3<f32> {
    let aspect = u.resolution.x / u.resolution.y;
    var color = mix(u.color_a.rgb * 0.25, u.color_a.rgb * 0.6, uv.y);
    let p = vec2<f32>(uv.x * aspect, uv.y);
    for (var layer = 0; layer < 2; layer = layer + 1) {
        let fl = f32(layer);
        let scale = 5.0 + fl * 3.0;
        let drift = vec2<f32>(0.0, t * (0.03 + 0.02 * fl));
        let q = p * scale + drift * scale;
        let cell = floor(q);
        // The light in this cell and its neighbors (lights spill across edges).
        for (var dy = -1; dy <= 1; dy = dy + 1) {
            for (var dx = -1; dx <= 1; dx = dx + 1) {
                let c = cell + vec2<f32>(f32(dx), f32(dy));
                let h = hash(c + fl * 17.0);
                if (h > 0.55) {
                    continue;
                }
                let center = c + vec2<f32>(hash(c + 3.1), hash(c + 7.7));
                let radius = 0.25 + 0.35 * hash(c + 11.3);
                let d = length(q - center);
                let disc = 1.0 - smoothstep(radius * 0.7, radius, d);
                let twinkle = 0.6 + 0.4 * sin(t * 0.8 + h * 40.0);
                let tint = mix(u.color_b.rgb, mix(u.color_b.rgb, vec3<f32>(1.0), 0.4), hash(c + 5.0));
                color = color + tint * disc * twinkle * (0.22 - 0.06 * fl);
            }
        }
    }
    return min(color, vec3<f32>(1.0));
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    // Origin at the bottom, so "up" in the presets is up on screen.
    let uv = vec2<f32>(position.x / u.resolution.x, 1.0 - position.y / u.resolution.y);
    var color: vec3<f32>;
    if (u.preset == 1u) {
        color = aurora(uv, u.time);
    } else if (u.preset == 2u) {
        color = bokeh(uv, u.time);
    } else {
        color = gradient(uv, u.time);
    }
    return vec4<f32>(color, 1.0);
}
