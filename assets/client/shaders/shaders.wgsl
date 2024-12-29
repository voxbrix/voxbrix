const BLOCKS_IN_CHUNK_EDGE_F32: f32 = 16.0;
const MAX_LIGHT_LEVEL_F32: f32 = 16.0;

struct CameraUniform {
    chunk: vec3<i32>,
    _padding: u32,
    view_position: vec4<f32>,
    view_projection: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexDescription {
    @location(0) index: u32,
};

struct QuadInput {
    @location(1) chunk: vec3<i32>,
    @location(2) texture_index: u32,
    @location(3) vertex_0_position: vec3<f32>,
    @location(4) vertex_0_texture_position: vec2<f32>,
    @location(5) vertex_0_light_level: u32,
    @location(6) vertex_1_position: vec3<f32>,
    @location(7) vertex_1_texture_position: vec2<f32>,
    @location(8) vertex_1_light_level: u32,
    @location(9) vertex_2_position: vec3<f32>,
    @location(10) vertex_2_texture_position: vec2<f32>,
    @location(11) vertex_2_light_level: u32,
    @location(12) vertex_3_position: vec3<f32>,
    @location(13) vertex_3_texture_position: vec2<f32>,
    @location(14) vertex_3_light_level: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) texture_index: u32,
    @location(1) texture_position: vec2<f32>,
    @location(2) sky_light_level: f32,
};

@vertex
fn vs_main(
    vertex_desc: VertexDescription,
    quad: QuadInput,
) -> VertexOutput {
    var out: VertexOutput;

    var position_array: array<vec3<f32>, 4> = array(
        quad.vertex_0_position,
        quad.vertex_1_position,
        quad.vertex_2_position,
        quad.vertex_3_position,
    );

    var texture_position_array: array<vec2<f32>, 4> = array(
        quad.vertex_0_texture_position,
        quad.vertex_1_texture_position,
        quad.vertex_2_texture_position,
        quad.vertex_3_texture_position,
    );

    var light_level_array: array<u32, 4> = array(
        quad.vertex_0_light_level,
        quad.vertex_1_light_level,
        quad.vertex_2_light_level,
        quad.vertex_3_light_level,
    );

    let position = vec3<f32>(quad.chunk - camera.chunk)
        * BLOCKS_IN_CHUNK_EDGE_F32
        + position_array[vertex_desc.index];

    out.texture_position = texture_position_array[vertex_desc.index];

    out.clip_position = camera.view_projection * vec4<f32>(position, 1.0);
    
    out.texture_index = quad.texture_index;

    let sky_light_level: u32 = light_level_array[vertex_desc.index] >> 0u & 0xFFu;
    out.sky_light_level = f32(sky_light_level) / MAX_LIGHT_LEVEL_F32;
    out.sky_light_level = pow(out.sky_light_level, 1.5);

    // let light_r: u32 = in.joints >>  8u & 0xFFu;
    // let light_g: u32 = in.joints >> 16u & 0xFFu;
    // let light_b: u32 = in.joints >> 24u & 0xFFu;
    
    return out;
}


@group(1) @binding(0)
var textures: texture_2d_array<u32>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var dimensions = textureDimensions(textures);

    var texture_position = vec2<u32>(vec2<f32>(dimensions) * in.texture_position);

    var uint_output = textureLoad(
        textures,
        texture_position,
        in.texture_index,
        0
    );

    var output: vec4<f32> = vec4<f32>(uint_output) / 255.0;

    output[0] *= in.sky_light_level;
    output[1] *= in.sky_light_level;
    output[2] *= in.sky_light_level;

    return output;
}
