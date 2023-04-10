use anyhow::Error;
use image::ImageFormat;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    num::NonZeroU32,
    path::Path,
};

const PATH: &str = "assets/client/textures/blocks";
const LIST_FILE_NAME: &str = "list.ron";
const BLOCK_TEXTURE_FORMAT: ImageFormat = ImageFormat::Png;
const BLOCK_TEXTURE_FORMAT_NAME: &str = "png";
const GPU_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

pub struct BlockTextureLoadingSystem {
    pub size: usize,
    pub textures: Vec<Vec<u8>>,
    pub label_map: BTreeMap<String, u32>,
}

impl BlockTextureLoadingSystem {
    pub async fn load_data() -> Result<Self, Error> {
        blocking::unblock(|| {
            let texture_list = {
                let path = Path::new(PATH).join(LIST_FILE_NAME);
                let string = fs::read_to_string(path)?;
                ron::from_str::<BlockTextureList>(&string)?
            };

            let textures = texture_list
                .list
                .iter()
                .map(|block_texture_label| {
                    let file_name =
                        format!("{}.{}", block_texture_label, BLOCK_TEXTURE_FORMAT_NAME);
                    let path = Path::new(PATH).join(file_name);
                    fs::read(path)
                })
                .collect::<Result<Vec<_>, _>>()?;

            let label_map = texture_list
                .list
                .into_iter()
                .enumerate()
                .map(|(i, t)| (t, i as u32))
                .collect();

            Ok(Self {
                size: texture_list.size,
                textures,
                label_map,
            })
        })
        .await
    }

    pub fn prepare_buffer(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
        let textures = &self.textures;

        let block_texture_bytes = textures
            .iter()
            .map(|buf| {
                let bytes_rgba =
                    image::load_from_memory_with_format(buf.as_ref(), BLOCK_TEXTURE_FORMAT)?
                        .into_rgba8();

                Ok(bytes_rgba)
            })
            .collect::<Result<Vec<_>, anyhow::Error>>()
            .unwrap();

        // TODO
        let texture_size = block_texture_bytes[0].dimensions();

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
            format: GPU_TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("block_texture"),
            view_formats: &[GPU_TEXTURE_FORMAT],
        };

        let block_texture_views = block_texture_bytes
            .into_iter()
            .map(|texture_bytes| {
                let block_texture = device.create_texture(&texture_descriptior);
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &block_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &texture_bytes,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: NonZeroU32::new(4 * texture_size.0),
                        rows_per_image: NonZeroU32::new(texture_size.1),
                    },
                    extent,
                );
                block_texture.create_view(&wgpu::TextureViewDescriptor::default())
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

        let block_texture_views = block_texture_views.iter().collect::<Vec<_>>();

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("block_texture_bind_group_layout"),
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
                    resource: wgpu::BindingResource::TextureViewArray(
                        block_texture_views.as_slice(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::SamplerArray(
                        &textures.iter().map(|_| &sampler).collect::<Vec<_>>(),
                    ),
                },
            ],
            layout: &texture_bind_group_layout,
            label: Some("block_texture_bind_group"),
        });

        (texture_bind_group_layout, texture_bind_group)
    }
}

#[derive(Deserialize, Debug)]
struct BlockTextureList {
    size: usize,
    list: Vec<String>,
}
