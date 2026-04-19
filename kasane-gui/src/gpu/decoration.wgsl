// Text decoration shader for kasane-gui.
// Renders underlines (solid, curly, double) and strikethrough lines.
//
// Each instance represents a decoration line segment with a type parameter
// that controls the fragment shader output shape.

struct Uniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    // Local coordinates within the rect: (0..w, 0..h)
    @location(1) local_pos: vec2<f32>,
    // Rect dimensions (w, h) for fragment shader calculations
    @location(2) rect_size: vec2<f32>,
    // Decoration type (passed as f32 for interpolation-free transport)
    @location(3) @interpolate(flat) deco_type: u32,
}

// Instance layout: 10 floats
// [0..4] rect: x, y, w, h (pixels)
// [4..8] color: r, g, b, a (sRGB)
// [8]    decoration type: 0=solid, 1=curly, 2=double, 3=dotted, 4=dashed
// [9]    stroke thickness (pixels)
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
    @location(1) color: vec4<f32>,
    @location(2) params: vec2<f32>,
) -> VertexOutput {
    let w = rect.z;
    let h = rect.w;

    // Triangle strip: 0=TL, 1=TR, 2=BL, 3=BR
    let lx = select(0.0, w, (vertex_index & 1u) != 0u);
    let ly = select(0.0, h, (vertex_index & 2u) != 0u);

    let px = rect.x + lx;
    let py = rect.y + ly;

    // Pixel → NDC
    let ndc_x = (px / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / uniforms.screen_size.y) * 2.0;

    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = srgb_color_to_linear(color);
    out.local_pos = vec2<f32>(lx, ly);
    out.rect_size = vec2<f32>(w, h);
    out.deco_type = u32(params.x);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let lx = in.local_pos.x;
    let ly = in.local_pos.y;
    let w = in.rect_size.x;
    let h = in.rect_size.y;

    switch in.deco_type {
        // Solid line — full fill (used for standard underline and strikethrough)
        case 0u: {
            return in.color;
        }
        // Curly underline — sine wave
        case 1u: {
            // Wave parameters: one full cycle per `h * 2.0` pixels of width,
            // amplitude fills the rect height.
            let wavelength = h * 2.5;
            let amplitude = h * 0.5;
            let center_y = h * 0.5;

            // Sine wave: y_wave = center + amplitude * sin(2π * x / wavelength)
            let phase = (lx / wavelength) * 6.283185;
            let wave_y = center_y + amplitude * sin(phase);

            // Distance from the wave center line
            let dist = abs(ly - wave_y);
            // Anti-aliased stroke: 1px feather
            let stroke_w = max(h * 0.18, 1.0);
            let alpha = 1.0 - smoothstep(stroke_w * 0.5, stroke_w * 0.5 + 1.0, dist);

            if alpha < 0.01 {
                discard;
            }
            return vec4<f32>(in.color.rgb, in.color.a * alpha);
        }
        // Double underline — two parallel lines
        case 2u: {
            // Two lines: at 20% and 80% of the rect height, each ~1px thick
            let line_thickness = max(h * 0.2, 1.0);
            let line1_center = h * 0.15;
            let line2_center = h * 0.85;

            let d1 = abs(ly - line1_center);
            let d2 = abs(ly - line2_center);
            let d = min(d1, d2);

            let alpha = 1.0 - smoothstep(line_thickness * 0.5, line_thickness * 0.5 + 0.5, d);

            if alpha < 0.01 {
                discard;
            }
            return vec4<f32>(in.color.rgb, in.color.a * alpha);
        }
        // Dotted underline — evenly spaced dots
        case 3u: {
            let dot_spacing = max(h * 2.0, 4.0);
            let dot_radius = max(h * 0.35, 1.0);
            let center_y = h * 0.5;

            // Repeat along x within each dot_spacing cell
            let cell_x = lx % dot_spacing;
            let dot_center_x = dot_spacing * 0.5;

            let dx = cell_x - dot_center_x;
            let dy = ly - center_y;
            let dist = sqrt(dx * dx + dy * dy);

            let alpha = 1.0 - smoothstep(dot_radius - 0.5, dot_radius + 0.5, dist);

            if alpha < 0.01 {
                discard;
            }
            return vec4<f32>(in.color.rgb, in.color.a * alpha);
        }
        // Dashed underline — alternating dash and gap
        case 4u: {
            let period = max(h * 6.0, 8.0);
            let dash_ratio = 0.6; // 60% dash, 40% gap
            let center_y = h * 0.5;
            let stroke_h = max(h * 0.4, 1.0);

            let phase = (lx % period) / period;

            // Vertical distance from center
            let dy = abs(ly - center_y);
            let y_alpha = 1.0 - smoothstep(stroke_h * 0.5, stroke_h * 0.5 + 0.5, dy);

            // Horizontal dash/gap transition with AA
            let edge = dash_ratio;
            let aa_width = 1.0 / period; // 1px feather in normalized space
            let x_alpha = 1.0 - smoothstep(edge - aa_width, edge + aa_width, phase);

            let alpha = x_alpha * y_alpha;

            if alpha < 0.01 {
                discard;
            }
            return vec4<f32>(in.color.rgb, in.color.a * alpha);
        }
        default: {
            return in.color;
        }
    }
}
