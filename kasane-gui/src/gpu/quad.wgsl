// Unified quad shader for kasane-gui.
// Handles solid backgrounds, rounded rect borders, decorations, and gradients.
// Colors are received in linear space (CPU-side sRGB→linear conversion).
//
// Quad type is encoded in params.z:
//   0 = solid background
//   1 = rounded rect (border/shadow)
//   2 = decoration (underline/curly/double/dotted/dashed)
//   3 = gradient

struct Uniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) fill_color: vec4<f32>,
    @location(1) border_color: vec4<f32>,
    @location(2) end_color: vec4<f32>,
    @location(3) local_pos: vec2<f32>,
    @location(4) rect_size: vec2<f32>,
    @location(5) @interpolate(flat) params: vec4<f32>,
}

// Instance layout: 20 floats = 80 bytes
// [0..4]   rect: x, y, w, h (pixels)
// [4..8]   fill_color: r, g, b, a (linear)
// [8..12]  border_color: r, g, b, a (linear) — or gradient end_color
// [12..16] params: corner_radius, border_width, quad_type, deco_type
// [16..20] extra: reserved

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) rect: vec4<f32>,
    @location(1) fill_color: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) params: vec4<f32>,
    @location(4) extra: vec4<f32>,
) -> VertexOutput {
    let w = rect.z;
    let h = rect.w;

    // Triangle strip: 0=TL, 1=TR, 2=BL, 3=BR
    let u = select(0.0, 1.0, (vertex_index & 1u) != 0u);
    let v = select(0.0, 1.0, (vertex_index & 2u) != 0u);

    let px = rect.x + u * w;
    let py = rect.y + v * h;

    // Pixel → NDC
    let ndc_x = (px / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / uniforms.screen_size.y) * 2.0;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.fill_color = fill_color;
    out.border_color = border_color;
    out.end_color = extra; // Used for gradient end_color
    out.local_pos = vec2<f32>(u * w, v * h);
    out.rect_size = vec2<f32>(w, h);
    out.params = params;
    return out;
}

// SDF for a rounded rectangle centered at origin with half-extents `h` and corner radius `r`.
fn sdf_rounded_rect(p: vec2<f32>, half_ext: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_ext + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let quad_type = u32(in.params.z);

    switch quad_type {
        // Type 0: Solid background fill
        case 0u: {
            return in.fill_color;
        }

        // Type 1: Rounded rect (border/shadow)
        case 1u: {
            let center = in.rect_size * 0.5;
            let p = in.local_pos - center;
            let half_ext = center;
            let r = in.params.x;
            let bw = in.params.y;

            let d = sdf_rounded_rect(p, half_ext, r);
            let fill_alpha = 1.0 - smoothstep(-0.5, 0.5, d);

            if bw <= 0.0 {
                if r > 1.0 {
                    let shadow_alpha = smoothstep(0.0, r, -d);
                    if shadow_alpha < 0.001 { discard; }
                    return vec4<f32>(in.fill_color.rgb, in.fill_color.a * shadow_alpha);
                }
                return vec4<f32>(in.fill_color.rgb, in.fill_color.a * fill_alpha);
            }

            let inner_d = d + bw;
            let border_alpha = fill_alpha * (1.0 - smoothstep(-0.5, 0.5, -inner_d));
            let interior_alpha = 1.0 - smoothstep(-0.5, 0.5, inner_d);

            let fill = vec4<f32>(in.fill_color.rgb, in.fill_color.a * interior_alpha);
            let border = vec4<f32>(in.border_color.rgb, in.border_color.a * border_alpha);

            let out_a = border.a + fill.a * (1.0 - border.a);
            if out_a < 0.001 { discard; }
            let out_rgb = (border.rgb * border.a + fill.rgb * fill.a * (1.0 - border.a)) / out_a;
            return vec4<f32>(out_rgb, out_a);
        }

        // Type 2: Decoration (underline, curly, double, dotted, dashed)
        case 2u: {
            let lx = in.local_pos.x;
            let ly = in.local_pos.y;
            let w = in.rect_size.x;
            let h = in.rect_size.y;
            let deco = u32(in.params.w);

            switch deco {
                case 0u: { return in.fill_color; }
                case 1u: {
                    let wavelength = h * 2.5;
                    let amplitude = h * 0.5;
                    let center_y = h * 0.5;
                    let phase = (lx / wavelength) * 6.283185;
                    let wave_y = center_y + amplitude * sin(phase);
                    let dist = abs(ly - wave_y);
                    let stroke_w = max(h * 0.18, 1.0);
                    let alpha = 1.0 - smoothstep(stroke_w * 0.5, stroke_w * 0.5 + 1.0, dist);
                    if alpha < 0.01 { discard; }
                    return vec4<f32>(in.fill_color.rgb, in.fill_color.a * alpha);
                }
                case 2u: {
                    let line_thickness = max(h * 0.2, 1.0);
                    let line1_center = h * 0.15;
                    let line2_center = h * 0.85;
                    let d1 = abs(ly - line1_center);
                    let d2 = abs(ly - line2_center);
                    let d = min(d1, d2);
                    let alpha = 1.0 - smoothstep(line_thickness * 0.5, line_thickness * 0.5 + 0.5, d);
                    if alpha < 0.01 { discard; }
                    return vec4<f32>(in.fill_color.rgb, in.fill_color.a * alpha);
                }
                case 3u: {
                    let dot_spacing = max(h * 2.0, 4.0);
                    let dot_radius = max(h * 0.35, 1.0);
                    let center_y = h * 0.5;
                    let cell_x = lx % dot_spacing;
                    let dot_center_x = dot_spacing * 0.5;
                    let dx = cell_x - dot_center_x;
                    let dy = ly - center_y;
                    let dist = sqrt(dx * dx + dy * dy);
                    let alpha = 1.0 - smoothstep(dot_radius - 0.5, dot_radius + 0.5, dist);
                    if alpha < 0.01 { discard; }
                    return vec4<f32>(in.fill_color.rgb, in.fill_color.a * alpha);
                }
                case 4u: {
                    let period = max(h * 6.0, 8.0);
                    let center_y = h * 0.5;
                    let stroke_h = max(h * 0.4, 1.0);
                    let phase = (lx % period) / period;
                    let dy = abs(ly - center_y);
                    let y_alpha = 1.0 - smoothstep(stroke_h * 0.5, stroke_h * 0.5 + 0.5, dy);
                    let edge = 0.6;
                    let aa_width = 1.0 / period;
                    let x_alpha = 1.0 - smoothstep(edge - aa_width, edge + aa_width, phase);
                    let alpha = x_alpha * y_alpha;
                    if alpha < 0.01 { discard; }
                    return vec4<f32>(in.fill_color.rgb, in.fill_color.a * alpha);
                }
                default: { return in.fill_color; }
            }
        }

        // Type 3: Gradient (vertical)
        case 3u: {
            let t = in.local_pos.y / in.rect_size.y;
            var color = mix(in.fill_color, in.end_color, t);

            // Ordered dithering (4x4 Bayer)
            let px = vec2<u32>(u32(in.position.x) % 4u, u32(in.position.y) % 4u);
            let idx = px.y * 4u + px.x;
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

        default: {
            return in.fill_color;
        }
    }
}
