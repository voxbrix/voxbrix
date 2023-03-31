let BLOCKS_IN_CHUNK_EDGE_F32: f32 = 32.0;
let MAX_LIGHT_LEVEL_F32: f32 = 16.0;

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
    @location(4) light_level: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) texture_index: u32,
    @location(1) texture_position: vec2<f32>,
    @location(2) sky_light_level: f32,
};

@vertex
fn vs_main(
    in: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    let position = vec3<f32>(in.chunk - camera.chunk) * BLOCKS_IN_CHUNK_EDGE_F32 + in.position;
    out.clip_position = camera.view_projection * vec4<f32>(position, 1.0);
    
    out.texture_index = in.texture_index;
    out.texture_position = in.texture_position;

    let sky_light_level: u32 = in.light_level >> 0u & 0xFFu;
    out.sky_light_level = f32(sky_light_level) / MAX_LIGHT_LEVEL_F32;
    out.sky_light_level = pow(out.sky_light_level, 3.0);
    
    return out;
}


@group(0) @binding(0)
var block_textures: binding_array<texture_2d<f32>>;
@group(0) @binding(1)
var block_samplers: binding_array<sampler>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var output = textureSample(
        block_textures[in.texture_index],
        block_samplers[in.texture_index],
        in.texture_position
    );

    output[0] *= in.sky_light_level;
    output[1] *= in.sky_light_level;
    output[2] *= in.sky_light_level;

    return output;
}
