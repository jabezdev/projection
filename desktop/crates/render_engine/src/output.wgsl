struct Params {
    // xy: displayed frame half-extent in NDC (aspect-correct contain fit).
    fit: vec4<f32>,
    // Letterbox color, distinct per output role.
    letterbox: vec4<f32>,
    // x: seconds on this output's own clock (pacing indicator);
    // yz: frame size as a fraction of the texture; w: 1 when a frame is shown.
    info: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var frame_texture: texture_2d<f32>;
@group(0) @binding(2) var frame_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

// One oversized triangle covering the viewport; no vertex buffer.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    let ndc = uv * 2.0 - 1.0;
    var out: VsOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.ndc = ndc;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Per-output marker sweeping along the bottom edge: stutter here means this
    // output's presentation stalled, independent of the shared frame.
    if (in.ndc.y < -0.96) {
        let marker = fract(params.info.x * 0.5) * 2.0 - 1.0;
        if (abs(in.ndc.x - marker) < 0.015) {
            return vec4<f32>(1.0, 0.2, 0.2, 1.0);
        }
    }
    let rel = in.ndc / params.fit.xy;
    if (params.info.w < 0.5 || abs(rel.x) > 1.0 || abs(rel.y) > 1.0) {
        return params.letterbox;
    }
    // The frame occupies the texture's top-left corner.
    let uv = vec2<f32>(rel.x * 0.5 + 0.5, 0.5 - rel.y * 0.5) * params.info.yz;
    return textureSampleLevel(frame_texture, frame_sampler, uv, 0.0);
}
