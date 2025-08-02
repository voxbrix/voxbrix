const BLOCKS_IN_CHUNK_EDGE_F32: f32 = 32.0;
const MAX_LIGHT_LEVEL_F32: f32 = 16.0;

struct CameraUniform {
    chunk: vec3<i32>,
    animation_timer: u32,
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


struct TextureParameters {
    mpf_interp: u32,
};

@group(1) @binding(0)
var textures: binding_array<texture_2d_array<u32>>;

@group(1) @binding(1)
var<storage, read> texture_parameters: array<TextureParameters>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dimensions = textureDimensions(textures[in.texture_index]);
    let layers = textureNumLayers(textures[in.texture_index]);

    let texture_position = vec2<u32>(vec2<f32>(dimensions) * in.texture_position);

    let parameters = texture_parameters[in.texture_index];

    let ms_per_frame = (parameters.mpf_interp >> 1) & 0xFFFF;

    let layer_index = camera.animation_timer / ms_per_frame % layers;

    let uint_output = textureLoad(
        textures[in.texture_index],
        texture_position,
        layer_index,
        0
    );

    if uint_output[3] == 0 {
        discard;
    }

    // 0.0 or 1.0 for false or true
    let interpolate = f32(parameters.mpf_interp & 0x1);

    let layer_index_next = (layer_index + 1) % layers;

    let interp_coef_next = interpolate * f32(camera.animation_timer % ms_per_frame) / f32(ms_per_frame);
    let interp_coef = 1.0 - interp_coef_next;

    let uint_output_next = textureLoad(
        textures[in.texture_index],
        texture_position,
        layer_index_next,
        0
    );

    var output = vec3<f32>(uint_output.xyz) * interp_coef;
    let output_next = vec3<f32>(uint_output_next.xyz) * interp_coef_next;

    output += output_next;
    output /= 255.0;

    // Alpha channel of texture has 2 special values:
    // * 0 for fully transparent pixel that is discarded above; 
    // * 255 for fully opaque non-emissive texture.
    // All other alpha values mean emissive texture:
    // * 1 for minimal emission;
    // * 254 for maximum emission.
    let emission = f32(uint_output[3] % 255) / 254.0;

    let sky_light_coef = min(emission + in.sky_light_level, 1.0);

    output *= sky_light_coef;

    return vec4<f32>(output, 1.0);
}
