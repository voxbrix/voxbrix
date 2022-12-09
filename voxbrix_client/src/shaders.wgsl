struct CameraUniform {
    chunk: vec3<i32>,
    _padding: u32,
    view_position: vec4<f32>,
    view_projection: mat4x4<f32>,
};

@group(1) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) chunk: vec3<i32>,
    @location(1) position: vec3<f32>,
    @location(2) texture_index: u32,
    @location(3) texture_position: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) texture_index: u32,
    @location(1) texture_position: vec2<f32>,
};

@vertex
fn vs_main(
    in: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    let position = vec3<f32>(in.chunk - camera.chunk) * 16.0 + in.position;
    out.clip_position = camera.view_projection * vec4<f32>(position, 1.0);
    out.texture_index = in.texture_index;
    out.texture_position = in.texture_position;
    return out;
}


@group(0) @binding(0)
var block_textures: binding_array<texture_2d<f32>>;
@group(0) @binding(1)
var block_samplers: binding_array<sampler>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(block_textures[in.texture_index], block_samplers[in.texture_index], in.texture_position);
}
