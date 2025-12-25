@vertex
fn vs_main(@builtin(vertex_index) index: u32)
    -> @builtin(position) vec4<f32> {

    let pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(-1.0,  3.0),
        vec2<f32>( 3.0, -1.0),
    );

    return vec4<f32>(pos[index], 0.0, 1.0);
}

@group(0) @binding(0)
var overlay: texture_2d<u32>;

@fragment
fn fs_main(@builtin(position) position: vec4<f32>)
    -> @location(0) vec4<f32> {
    let uint_channels = textureLoad(overlay, vec2<u32>(position.xy), 0);

    return vec4<f32>(uint_channels) / 255.0;
}
