// Gradient background shader for kasane-gui.
// Renders a fullscreen gradient quad with vertical interpolation and dithering.

struct Uniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) start_color: vec4<f32>,
    @location(1) end_color: vec4<f32>,
    // UV coordinate for gradient interpolation (0..1 vertical)
    @location(2) uv: vec2<f32>,
}

// Instance layout: 12 floats
// [0..4]  rect: x, y, w, h (pixels)
// [4..8]  start_color: r, g, b, a (sRGB)
// [8..12] end_color: r, g, b, a (sRGB)

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

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) rect: vec4<f32>,
    @location(1) start_color: vec4<f32>,
    @location(2) end_color: vec4<f32>,
) -> VertexOutput {
    let w = rect.z;
    let h = rect.w;

    // Triangle strip: 0=TL, 1=TR, 2=BL, 3=BR
    let lx = select(0.0, w, (vertex_index & 1u) != 0u);
    let ly = select(0.0, h, (vertex_index & 2u) != 0u);

    let px = rect.x + lx;
    let py = rect.y + ly;

    // Pixel -> NDC
    let ndc_x = (px / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / uniforms.screen_size.y) * 2.0;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.start_color = srgb_color_to_linear(start_color);
    out.end_color = srgb_color_to_linear(end_color);
    out.uv = vec2<f32>(lx / w, ly / h);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = in.uv.y;
    var color = mix(in.start_color, in.end_color, t);

    // Ordered dithering to prevent banding on subtle gradients.
    // 4x4 Bayer matrix normalized to [-0.5, 0.5] / 255.
    let px = vec2<u32>(u32(in.position.x) % 4u, u32(in.position.y) % 4u);
    let idx = px.y * 4u + px.x;

    // Bayer 4x4 thresholds (0..15) mapped to [-0.5/255, 0.5/255]
    var threshold: array<f32, 16> = array<f32, 16>(
         0.0,  8.0,  2.0, 10.0,
        12.0,  4.0, 14.0,  6.0,
         3.0, 11.0,  1.0,  9.0,
        15.0,  7.0, 13.0,  5.0
    );
    let dither = (threshold[idx] / 16.0 - 0.5) / 255.0;
    color = vec4<f32>(color.rgb + vec3<f32>(dither), color.a);

    return color;
}
