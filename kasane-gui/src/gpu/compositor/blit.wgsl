// Full-screen blit shader with opacity.
// Renders a texture onto the framebuffer using a full-screen triangle.

@group(0) @binding(0)
var t_source: texture_2d<f32>;
@group(0) @binding(1)
var s_source: sampler;

struct BlitParams {
    opacity: f32,
}

@group(1) @binding(0)
var<uniform> params: BlitParams;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Full-screen triangle (3 vertices, no vertex buffer needed)
    var out: VertexOutput;
    let x = f32(i32(vertex_index & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vertex_index >> 1u)) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    // NDC → UV: x: [-1,1] → [0,1], y: [-1,1] → [1,0] (flip Y)
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_source, s_source, in.uv);
    return vec4<f32>(color.rgb, color.a * params.opacity);
}
