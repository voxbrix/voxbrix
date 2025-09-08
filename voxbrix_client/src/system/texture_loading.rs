use crate::entity::texture::Texture;
use anyhow::{
    Context,
    Error,
};
use image::GenericImageView;
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    mem,
    num::NonZero,
    path::Path,
};
use tokio::task;
use voxbrix_common::{
    read_data_file,
    LabelMap,
};
use wgpu::util::DeviceExt;

const TEXTURE_FORMAT_NAME: &str = "png";
const TEXTURE_FORMAT_SHADER: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Uint;
const TEXTURE_BYTES_PER_PIXEL: u32 = 4;

#[derive(Clone, Copy, Deserialize, Debug)]
struct AnimationParameters {
    frames: NonZero<u32>,
    ms_per_frame: NonZero<u16>,
    interpolate: bool,
}

impl Default for AnimationParameters {
    fn default() -> Self {
        Self {
            frames: NonZero::new(1).unwrap(),
            ms_per_frame: NonZero::new(1).unwrap(),
            interpolate: false,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct TextureParameters {
    // `ms_per_frame` and `interpolate` encoded into one u32 using bitshift,
    // from most significant to least significant:
    // first 15 reserved bits,
    // secont 16 bits is `ms_per_frame`,
    // then 1 bit for `interpolate`:
    mpf_interp: u32,
}

impl TextureParameters {
    pub fn new(anim: &AnimationParameters) -> Self {
        let mpf_interp: u32 = ((anim.ms_per_frame.get() as u32) << 1) | (anim.interpolate as u32);

        Self { mpf_interp }
    }
}

#[derive(Deserialize, Debug)]
struct AnimationMap(HashMap<String, AnimationParameters>);

pub struct TextureLoadingSystem {
    label_map: LabelMap<Texture>,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

impl TextureLoadingSystem {
    pub fn label_map(&self) -> LabelMap<Texture> {
        self.label_map.clone()
    }

    pub fn bind_group(&self) -> wgpu::BindGroup {
        self.bind_group.clone()
    }

    pub fn bind_group_layout(&self) -> wgpu::BindGroupLayout {
        self.bind_group_layout.clone()
    }

    pub async fn load_data(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list_path: &'static str,
        path_prefix: &'static str,
        animation_path: &'static str,
    ) -> Result<Self, Error> {
        let texture_list: Vec<String> = task::spawn_blocking(move || read_data_file(list_path))
            .await
            .unwrap()?;

        let label_map = LabelMap::from_list(&texture_list);

        let animation_map: AnimationMap =
            task::spawn_blocking(move || read_data_file(animation_path))
                .await
                .unwrap()?;

        let (texture_views, texture_parameters) = {
            let device = device.clone();
            let queue = queue.clone();

            task::spawn_blocking(move || {
                let mut views = Vec::new();
                let mut parameters = Vec::new();

                for texture_label in texture_list.into_iter() {
                    let file_path = Path::new(path_prefix)
                        .join(format!("{}.{}", texture_label, TEXTURE_FORMAT_NAME));

                    let image_file = fs::read(&file_path)
                        .with_context(|| format!("reading texture file {:?}", &file_path))?;

                    let image = image::load_from_memory(&image_file)
                        .with_context(|| format!("reading image from {:?}", &file_path))?;

                    let anim_param = animation_map
                        .0
                        .get(&texture_label)
                        .copied()
                        .unwrap_or_default();

                    let layers = anim_param.frames.get();

                    let view = load_texture(&device, &queue, &image, &texture_label, layers)
                        .with_context(|| format!("loading texture {:?}", &file_path))?;

                    views.push(view);
                    parameters.push(TextureParameters::new(&anim_param));
                }

                Ok::<_, Error>((views, parameters))
            })
        }
        .await
        .unwrap()?;

        let count: u32 = texture_views.len().try_into().expect("too many textures");
        let count: NonZero<u32> = count
            .try_into()
            .with_context(|| format!("no textures provided in {:?}", &list_path))?;

        let texture_parameters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("texture_parameters_buffer"),
            usage: wgpu::BufferUsages::STORAGE,
            contents: bytemuck::cast_slice(texture_parameters.as_slice()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: Some(count),
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(
                            (count.get() as u64 * mem::size_of::<TextureParameters>() as u64)
                                .try_into()
                                .expect("texture parameters size must not be zero"),
                        ),
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(
                        &texture_views.iter().collect::<Vec<_>>(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &texture_parameters,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
            layout: &bind_group_layout,
            label: Some("texture_bind_group"),
        });

        Ok(Self {
            label_map,
            bind_group_layout,
            bind_group,
        })
    }
}

fn load_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &image::DynamicImage,
    texture_label: &str,
    layers: u32,
) -> Result<wgpu::TextureView, Error> {
    let image = image.to_rgba8();

    let layer_height = image.height() / layers;
    anyhow::ensure!(
        image.height() % layers == 0,
        "texture layer alignment failed, height: {}, layers: {}",
        image.height(),
        layers,
    );

    let texture_descriptior = wgpu::TextureDescriptor {
        size: wgpu::Extent3d {
            width: image.width(),
            height: layer_height,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT_SHADER,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        label: Some(texture_label),
        view_formats: &[TEXTURE_FORMAT_SHADER],
    };

    let texture = device.create_texture(&texture_descriptior);

    let texture_layer_size = wgpu::Extent3d {
        width: image.width(),
        height: layer_height,
        depth_or_array_layers: 1,
    };

    for z in 0 .. layers {
        let image = image
            .view(0, z * layer_height, image.width(), layer_height)
            .to_image();

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z },
                aspect: wgpu::TextureAspect::All,
            },
            &image.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TEXTURE_BYTES_PER_PIXEL * image.width()),
                rows_per_image: Some(image.height()),
            },
            texture_layer_size,
        );
    }

    queue.submit([]);

    Ok(texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    }))
}
