use anyhow::{
    Context,
    Error,
};
use image::ImageFormat;
use serde::Deserialize;
use std::{
    fs,
    num::NonZeroU32,
    path::Path,
};
use tokio::task;
use voxbrix_common::{
    read_ron_file,
    LabelMap,
};

const TEXTURE_FORMAT: ImageFormat = ImageFormat::Png;
const TEXTURE_FORMAT_NAME: &str = "png";
const TEXTURE_FORMAT_WGPU: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

#[derive(Deserialize, Debug)]
struct TextureList {
    size: usize,
    list: Vec<String>,
}

pub struct TextureLoadingSystem {
    pub size: usize,
    pub textures: Vec<Vec<u8>>,
    pub label_map: LabelMap<u32>,
}

impl TextureLoadingSystem {
    pub async fn load_data(
        list_path: &'static str,
        path_prefix: &'static str,
    ) -> Result<Self, Error> {
        task::spawn_blocking(move || {
            let texture_list: TextureList = read_ron_file(list_path)?;

            let textures = texture_list
                .list
                .iter()
                .map(|texture_label| {
                    let file_path = Path::new(path_prefix)
                        .join(format!("{}.{}", texture_label, TEXTURE_FORMAT_NAME));
                    fs::read(&file_path).with_context(|| format!("reading {:?}", &file_path))
                })
                .collect::<Result<Vec<_>, _>>()?;

            let label_map = LabelMap::from_list(&texture_list.list);

            Ok(Self {
                size: texture_list.size,
                textures,
                label_map,
            })
        })
        .await
        .unwrap()
    }

    pub fn prepare_buffer(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture_format: &wgpu::TextureFormat,
    ) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
        let textures = &self.textures;

        let texture_bytes = textures
            .iter()
            .map(|buf| {
                let bytes_rgba =
                    image::load_from_memory_with_format(buf.as_ref(), TEXTURE_FORMAT)?.into_rgba8();

                Ok(bytes_rgba)
            })
            .collect::<Result<Vec<_>, anyhow::Error>>()
            .unwrap();

        // TODO
        let texture_size = texture_bytes[0].dimensions();

        let extent = wgpu::Extent3d {
            width: texture_size.0,
            height: texture_size.1,
            depth_or_array_layers: 1,
        };

        let texture_descriptior = wgpu::TextureDescriptor {
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEXTURE_FORMAT_WGPU,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("texture"),
            view_formats: &[*texture_format],
        };

        let texture_views = texture_bytes
            .into_iter()
            .map(|texture_bytes| {
                let texture = device.create_texture(&texture_descriptior);
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &texture_bytes,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * texture_size.0),
                        rows_per_image: Some(texture_size.1),
                    },
                    extent,
                );
                texture.create_view(&wgpu::TextureViewDescriptor::default())
            })
            .collect::<Vec<_>>();

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture_views = texture_views.iter().collect::<Vec<_>>();

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: NonZeroU32::new(textures.len() as u32),
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: NonZeroU32::new(textures.len() as u32),
                    },
                ],
            });

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(texture_views.as_slice()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::SamplerArray(
                        &textures.iter().map(|_| &sampler).collect::<Vec<_>>(),
                    ),
                },
            ],
            layout: &texture_bind_group_layout,
            label: Some("texture_bind_group"),
        });

        (texture_bind_group_layout, texture_bind_group)
    }
}
