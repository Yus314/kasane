// Text glow post-process: reads source text texture alpha,
// applies multi-tap blur for glow effect, outputs glow color.

@group(0) @binding(0) var source_tex: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct Params {
    // x: glow radius in UV space, yzw: unused
    glow_params: vec4<f32>,
    glow_color: vec4<f32>,
}
@group(1) @binding(0) var<uniform> params: Params;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    let x = f32(i32(idx & 1u)) * 4.0 - 1.0;
    let y = f32(i32(idx >> 1u)) * 4.0 - 1.0;
    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let radius = params.glow_params.x;
    let uv = in.uv;

    // 9-tap box kernel for glow
    var alpha = 0.0;
    let d = radius;
    alpha += textureSample(source_tex, source_sampler, uv).a;
    alpha += textureSample(source_tex, source_sampler, uv + vec2<f32>(d, 0.0)).a;
    alpha += textureSample(source_tex, source_sampler, uv - vec2<f32>(d, 0.0)).a;
    alpha += textureSample(source_tex, source_sampler, uv + vec2<f32>(0.0, d)).a;
    alpha += textureSample(source_tex, source_sampler, uv - vec2<f32>(0.0, d)).a;
    alpha += textureSample(source_tex, source_sampler, uv + vec2<f32>(d, d)).a;
    alpha += textureSample(source_tex, source_sampler, uv + vec2<f32>(d, -d)).a;
    alpha += textureSample(source_tex, source_sampler, uv + vec2<f32>(-d, d)).a;
    alpha += textureSample(source_tex, source_sampler, uv + vec2<f32>(-d, -d)).a;
    alpha /= 9.0;

    let color = params.glow_color;
    return vec4<f32>(color.rgb, color.a * alpha);
}
