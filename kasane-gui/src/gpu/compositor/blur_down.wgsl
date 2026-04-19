// Dual-Filter Kawase blur: downsample pass.
// 4-tap cross pattern samples from the source at half resolution.

@group(0) @binding(0)
var t_source: texture_2d<f32>;
@group(0) @binding(1)
var s_source: sampler;

struct BlurParams {
    texel_size: vec2<f32>, // 1.0 / source_resolution
}

@group(1) @binding(0)
var<uniform> params: BlurParams;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vertex_index & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vertex_index >> 1u)) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let half = params.texel_size * 0.5;

    // 5-tap downsample (center + 4 diagonal neighbors)
    var color = textureSample(t_source, s_source, in.uv) * 4.0;
    color += textureSample(t_source, s_source, in.uv + vec2<f32>(-half.x, -half.y));
    color += textureSample(t_source, s_source, in.uv + vec2<f32>( half.x, -half.y));
    color += textureSample(t_source, s_source, in.uv + vec2<f32>(-half.x,  half.y));
    color += textureSample(t_source, s_source, in.uv + vec2<f32>( half.x,  half.y));

    return color / 8.0;
}
