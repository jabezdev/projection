// Scales an output's full-size frame down into a live monitor in the
// operator window. Each pixel averages taps × taps bilinear samples spread
// over its footprint, so text and borders stay smooth at a quarter size.
struct Params {
    // xy: one monitor pixel in source UV units; z: taps per axis.
    footprint: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// One oversized triangle covering the viewport; no vertex buffer.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VsOut;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let taps = i32(params.footprint.z);
    var sum = vec4<f32>(0.0);
    for (var i = 0; i < taps; i++) {
        for (var j = 0; j < taps; j++) {
            let offset = (vec2<f32>(f32(i), f32(j)) + 0.5) / f32(taps) - 0.5;
            sum += textureSampleLevel(source, source_sampler, in.uv + offset * params.footprint.xy, 0.0);
        }
    }
    return vec4<f32>(sum.rgb / f32(taps * taps), 1.0);
}
