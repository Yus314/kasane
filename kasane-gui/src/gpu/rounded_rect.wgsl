// SDF-based rounded rectangle shader for borders and shadows.
// Each instance is a rounded rect with optional border stroke.

struct Uniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,    // position within the rect (0..w, 0..h)
    @location(1) rect_size: vec2<f32>,    // (w, h) of the rect
    @location(2) corner_radius: f32,
    @location(3) border_width: f32,
    @location(4) fill_color: vec4<f32>,
    @location(5) border_color: vec4<f32>,
}

// Instance data: 14 floats
// [0..4] rect: x, y, w, h
// [4]    corner_radius
// [5]    border_width
// [6..10] fill_color: r, g, b, a
// [10..14] border_color: r, g, b, a

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
    @location(0) rect: vec4<f32>,           // x, y, w, h
    @location(1) params: vec2<f32>,         // corner_radius, border_width
    @location(2) fill_color: vec4<f32>,
    @location(3) border_color: vec4<f32>,
) -> VertexOutput {
    // Triangle strip: 4 vertices
    let u = select(0.0, 1.0, (vertex_index & 1u) != 0u);
    let v = select(0.0, 1.0, (vertex_index & 2u) != 0u);

    let x = rect.x + u * rect.z;
    let y = rect.y + v * rect.w;

    // NDC
    let ndc_x = (x / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (y / uniforms.screen_size.y) * 2.0;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.local_pos = vec2<f32>(u * rect.z, v * rect.w);
    out.rect_size = vec2<f32>(rect.z, rect.w);
    out.corner_radius = params.x;
    out.border_width = params.y;
    out.fill_color = srgb_color_to_linear(fill_color);
    out.border_color = srgb_color_to_linear(border_color);
    return out;
}

// SDF for a rounded rectangle centered at origin with half-extents `h` and corner radius `r`.
fn sdf_rounded_rect(p: vec2<f32>, half_ext: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_ext + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Center the coordinate system
    let center = in.rect_size * 0.5;
    let p = in.local_pos - center;
    let half_ext = center;
    let r = in.corner_radius;
    let bw = in.border_width;

    let d = sdf_rounded_rect(p, half_ext, r);

    // Anti-aliased fill
    let fill_alpha = 1.0 - smoothstep(-0.5, 0.5, d);

    if bw <= 0.0 {
        if r > 1.0 {
            // Shadow: smooth gradient falloff using SDF distance.
            // d < 0 inside the shape, d = 0 at the edge.
            // Fade from full opacity (d <= -r) to transparent (d >= 0).
            let shadow_alpha = smoothstep(0.0, r, -d);
            if shadow_alpha < 0.001 {
                discard;
            }
            return vec4<f32>(in.fill_color.rgb, in.fill_color.a * shadow_alpha);
        }
        // Solid fill (background rectangle)
        return vec4<f32>(in.fill_color.rgb, in.fill_color.a * fill_alpha);
    }

    // Border stroke: band around d = 0 with width bw
    let inner_d = d + bw;
    let border_alpha = fill_alpha * (1.0 - smoothstep(-0.5, 0.5, -inner_d));

    // Interior fill (inside the border)
    let interior_alpha = 1.0 - smoothstep(-0.5, 0.5, inner_d);

    // Composite: border on top of fill
    let fill = vec4<f32>(in.fill_color.rgb, in.fill_color.a * interior_alpha);
    let border = vec4<f32>(in.border_color.rgb, in.border_color.a * border_alpha);

    // Alpha blend: border over fill
    let out_a = border.a + fill.a * (1.0 - border.a);
    if out_a < 0.001 {
        discard;
    }
    let out_rgb = (border.rgb * border.a + fill.rgb * fill.a * (1.0 - border.a)) / out_a;
    return vec4<f32>(out_rgb, out_a);
}
