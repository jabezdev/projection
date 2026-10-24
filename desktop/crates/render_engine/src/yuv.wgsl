// Converts one decoded YUV frame into the shared RGBA frame. Output values
// stay gamma-encoded (sRGB-like), as the rest of the outputs expect. The
// planes may be larger than the output (4K into 1080p): linear sampling at
// output pixel centers scales them down.

struct Uniforms {
    // Each row: RGB channel = dot(row.xyz, yuv) + row.w.
    r: vec4<f32>,
    g: vec4<f32>,
    b: vec4<f32>,
    // Output size in pixels; x > 0.5 in `flags` means interleaved UV (NV12).
    size: vec2<f32>,
    flags: vec2<f32>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var y_plane: texture_2d<f32>;
@group(0) @binding(2) var u_plane: texture_2d<f32>;
@group(0) @binding(3) var v_plane: texture_2d<f32>;
@group(0) @binding(4) var planes: sampler;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy / u.size;
    let luma = textureSample(y_plane, planes, uv).r;
    let chroma = textureSample(u_plane, planes, uv);
    let cr = select(textureSample(v_plane, planes, uv).r, chroma.g, u.flags.x > 0.5);
    let yuv = vec3<f32>(luma, chroma.r, cr);
    let rgb = vec3<f32>(dot(u.r.xyz, yuv) + u.r.w, dot(u.g.xyz, yuv) + u.g.w, dot(u.b.xyz, yuv) + u.b.w);
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
