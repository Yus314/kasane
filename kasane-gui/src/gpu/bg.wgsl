// Background quad shader for kasane-gui.
// Each instance is a colored rectangle (cell background or cursor).

struct Uniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    } else {
        return pow((c + 0.055) / 1.055, 2.4);
    }
}

fn srgb_color_to_linear(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(srgb_to_linear(c.r), srgb_to_linear(c.g), srgb_to_linear(c.b), c.a);
}

// rect: (x, y, w, h) in pixels
// color: (r, g, b, a) in sRGB
@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
) -> VertexOutput {
    // Triangle strip: 4 vertices → 2 triangles
    // 0: top-left, 1: top-right, 2: bottom-left, 3: bottom-right
    let x = select(rect.x, rect.x + rect.z, (vertex_index & 1u) != 0u);
    let y = select(rect.y, rect.y + rect.w, (vertex_index & 2u) != 0u);

    // Convert pixel coordinates to NDC (-1..1)
    let ndc_x = (x / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (y / uniforms.screen_size.y) * 2.0;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = srgb_color_to_linear(color);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
