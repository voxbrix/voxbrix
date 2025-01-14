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
    @location(3) vertices_0: vec4<f32>,
    @location(4) vertices_1: vec4<f32>,
    @location(5) vertices_2: vec4<f32>,
    @location(6) texture_positions_0: vec4<f32>,
    @location(7) texture_positions_1: vec4<f32>,
    @location(8) light_parameters: vec4<u32>,
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
        vec3<f32>(quad.vertices_0[0], quad.vertices_0[1], quad.vertices_0[2]),
        vec3<f32>(quad.vertices_0[3], quad.vertices_1[0], quad.vertices_1[1]),
        vec3<f32>(quad.vertices_1[2], quad.vertices_1[3], quad.vertices_2[0]),
        vec3<f32>(quad.vertices_2[1], quad.vertices_2[2], quad.vertices_2[3]),
    );

    var texture_position_array: array<vec2<f32>, 4> = array(
        vec2<f32>(quad.texture_positions_0[0], quad.texture_positions_0[1]),
        vec2<f32>(quad.texture_positions_0[2], quad.texture_positions_0[3]),
        vec2<f32>(quad.texture_positions_1[0], quad.texture_positions_1[1]),
        vec2<f32>(quad.texture_positions_1[2], quad.texture_positions_1[3]),
    );

    let position = vec3<f32>(quad.chunk - camera.chunk)
        * BLOCKS_IN_CHUNK_EDGE_F32
        + position_array[vertex_desc.index];

    out.texture_position = texture_position_array[vertex_desc.index];

    out.clip_position = camera.view_projection * vec4<f32>(position, 1.0);
    
    out.texture_index = quad.texture_index;

    let sky_light_level: u32 = quad.light_parameters[vertex_desc.index] & 0xFFu;
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

    // Alpha channel is divided in two:
    //   * Lower 4 bits define opacity;
    //   * Higher 4 bits define how much the pixel is affected by shadow:
    //     0 - not affected (max glowing), 15 - fully affected (not glowing).
    var alpha = uint_output[3];

    var emission_coef: f32 = 1.0 - f32(alpha >> 4 & 15) / 15.0;

    let sky_light_coef = min(emission_coef + in.sky_light_level, 1.0);

    var output: vec4<f32> = vec4<f32>(uint_output) / 255.0;

    output[0] *= sky_light_coef;
    output[1] *= sky_light_coef;
    output[2] *= sky_light_coef;
    output[3] = f32(alpha & 15) / 15.0;

    return output;
}
