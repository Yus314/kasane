// Text shadow post-process: reads source text texture alpha,
// applies offset and gaussian-like blur, outputs shadow color.

@group(0) @binding(0) var source_tex: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct Params {
    // xy: shadow offset in UV space, z: blur radius in UV space, w: unused
    offset_blur: vec4<f32>,
    shadow_color: vec4<f32>,
}
@group(1) @binding(0) var<uniform> params: Params;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    // Full-screen triangle (3 vertices cover the screen)
    let x = f32(i32(idx & 1u)) * 4.0 - 1.0;
    let y = f32(i32(idx >> 1u)) * 4.0 - 1.0;
    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let offset = params.offset_blur.xy;
    let blur_radius = params.offset_blur.z;

    // Sample at offset position with simple 5-tap blur
    let uv = in.uv - offset;

    var alpha = 0.0;
    if blur_radius > 0.0001 {
        // 5-tap gaussian approximation
        let d = blur_radius;
        alpha += textureSample(source_tex, source_sampler, uv).a * 0.4;
        alpha += textureSample(source_tex, source_sampler, uv + vec2<f32>(d, 0.0)).a * 0.15;
        alpha += textureSample(source_tex, source_sampler, uv - vec2<f32>(d, 0.0)).a * 0.15;
        alpha += textureSample(source_tex, source_sampler, uv + vec2<f32>(0.0, d)).a * 0.15;
        alpha += textureSample(source_tex, source_sampler, uv - vec2<f32>(0.0, d)).a * 0.15;
    } else {
        alpha = textureSample(source_tex, source_sampler, uv).a;
    }

    let color = params.shadow_color;
    return vec4<f32>(color.rgb, color.a * alpha);
}
