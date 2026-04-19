// Dual-Filter Kawase blur: upsample pass.
// 8-tap ring pattern samples from the source at double resolution.

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
    let t = params.texel_size;
    let ht = t * 0.5;

    // 9-tap upsample (center weighted + 8 neighbors)
    var color = vec4<f32>(0.0);

    // Cardinal neighbors (weight 2 each)
    color += textureSample(t_source, s_source, in.uv + vec2<f32>(-t.x, 0.0)) * 2.0;
    color += textureSample(t_source, s_source, in.uv + vec2<f32>( t.x, 0.0)) * 2.0;
    color += textureSample(t_source, s_source, in.uv + vec2<f32>(0.0, -t.y)) * 2.0;
    color += textureSample(t_source, s_source, in.uv + vec2<f32>(0.0,  t.y)) * 2.0;

    // Diagonal neighbors (weight 1 each)
    color += textureSample(t_source, s_source, in.uv + vec2<f32>(-t.x, -t.y));
    color += textureSample(t_source, s_source, in.uv + vec2<f32>( t.x, -t.y));
    color += textureSample(t_source, s_source, in.uv + vec2<f32>(-t.x,  t.y));
    color += textureSample(t_source, s_source, in.uv + vec2<f32>( t.x,  t.y));

    return color / 12.0;
}
