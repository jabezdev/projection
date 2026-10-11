struct Globals {
    // xy: target size in pixels.
    size: vec4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var linear_sampler: sampler;
@group(1) @binding(0) var source: texture_2d<f32>;

struct Instance {
    // Local rectangle (x, y, w, h) in pixels, before the transform.
    @location(0) rect: vec4<f32>,
    // Texture coordinates (u0, v0, u1, v1).
    @location(1) uv: vec4<f32>,
    // Straight-alpha color, multiplied into the result.
    @location(2) color: vec4<f32>,
    // Transform: origin (x, y) in pixels, then cos and sin of the rotation.
    @location(3) transform: vec4<f32>,
    // x: 0 solid, 1 texture, 2 coverage mask.
    @location(4) kind: vec4<f32>,
}

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) kind: f32,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32, instance: Instance) -> VsOut {
    let corner = vec2<f32>(f32(index & 1u), f32((index >> 1u) & 1u));
    let local = instance.rect.xy + corner * instance.rect.zw;
    let c = instance.transform.z;
    let s = instance.transform.w;
    let world = instance.transform.xy + vec2<f32>(local.x * c - local.y * s, local.x * s + local.y * c);
    var out: VsOut;
    out.position = vec4<f32>(world.x / globals.size.x * 2.0 - 1.0, 1.0 - world.y / globals.size.y * 2.0, 0.0, 1.0);
    out.uv = mix(instance.uv.xy, instance.uv.zw, corner);
    out.color = instance.color;
    out.kind = instance.kind.x;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    if (in.kind < 0.5) {
        return in.color;
    }
    let texel = textureSampleLevel(source, linear_sampler, in.uv, 0.0);
    if (in.kind < 1.5) {
        return vec4<f32>(texel.rgb * in.color.rgb, texel.a * in.color.a);
    }
    return vec4<f32>(in.color.rgb, in.color.a * texel.r);
}
