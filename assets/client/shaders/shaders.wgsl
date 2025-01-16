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

struct VertexInput {
    @location(0) chunk: vec3<i32>,
    @location(1) texture_index: u32,
    @location(2) offset: vec3<f32>,
    @location(3) texture_position: vec2<f32>,
    @location(4) light_parameters: u32,
}

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

    let position = vec3<f32>(in.chunk - camera.chunk)
        * BLOCKS_IN_CHUNK_EDGE_F32
        + in.offset;

    out.texture_position = in.texture_position;

    out.clip_position = camera.view_projection * vec4<f32>(position, 1.0);
    
    out.texture_index = in.texture_index;

    let sky_light_level: u32 = in.light_parameters & 0xFFu;
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
