// Image quad shader for kasane-gui.
// Each instance is a textured rectangle with UV coordinates and opacity.

struct Uniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var t_image: texture_2d<f32>;
@group(1) @binding(1)
var s_image: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) opacity: f32,
}

// rect: (x, y, w, h) in pixels
// uv_rect: (u0, v0, u1, v1)
// opacity: alpha multiplier
@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) rect: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) opacity: f32,
) -> VertexOutput {
    // Triangle strip: 4 vertices → 2 triangles
    // 0: top-left, 1: top-right, 2: bottom-left, 3: bottom-right
    let lr = f32((vertex_index & 1u) != 0u);
    let tb = f32((vertex_index & 2u) != 0u);

    let x = rect.x + rect.z * lr;
    let y = rect.y + rect.w * tb;

    // Convert pixel coordinates to NDC (-1..1)
    let ndc_x = (x / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (y / uniforms.screen_size.y) * 2.0;

    let u = uv_rect.x + (uv_rect.z - uv_rect.x) * lr;
    let v = uv_rect.y + (uv_rect.w - uv_rect.y) * tb;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = vec2<f32>(u, v);
    out.opacity = opacity;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_image, s_image, in.uv);
    return vec4<f32>(color.rgb, color.a * in.opacity);
}
