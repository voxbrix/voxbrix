use crate::{
    component::texture::location::{
        Location,
        LocationTextureComponent,
    },
    entity::texture::Texture,
};
use anyhow::{
    Context,
    Error,
};
use image::{
    GenericImage,
    ImageFormat,
    RgbaImage,
};
use rect_packer::DensePacker;
use serde::Deserialize;
use std::{
    fs,
    num::NonZeroU32,
    path::Path,
};
use tokio::task;
use voxbrix_common::{
    read_data_file,
    LabelMap,
};

const TEXTURE_FORMAT: ImageFormat = ImageFormat::Png;
const TEXTURE_FORMAT_NAME: &str = "png";
const TEXTURE_FORMAT_WGPU: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const MIN_TEXTURE_ATLAS_SIZE: u32 = 512;
const MAX_TEXTURE_ATLAS_SIZE: u32 = 8096;
const EDGE_CORRECTION_PIXELS: f64 = 0.00001;

#[derive(Deserialize, Debug)]
struct TextureList {
    list: Vec<String>,
}

pub struct TextureLoadingSystem {
    data: Vec<RgbaImage>,
    label_map: LabelMap<Texture>,
}

impl TextureLoadingSystem {
    pub async fn load_data(
        device: &wgpu::Device,
        list_path: &'static str,
        path_prefix: &'static str,
        location_tc: &mut LocationTextureComponent,
    ) -> Result<Self, Error> {
        let max_size = device
            .limits()
            .max_texture_dimension_2d
            .min(MAX_TEXTURE_ATLAS_SIZE);

        let (locations, slf) = task::spawn_blocking(move || {
            let texture_list: TextureList = read_data_file(list_path)?;

            let mut packers = vec![DensePacker::new(
                MIN_TEXTURE_ATLAS_SIZE.try_into().unwrap(),
                MIN_TEXTURE_ATLAS_SIZE.try_into().unwrap(),
            )];

            let textures = texture_list
                .list
                .iter()
                .map(|texture_label| {
                    let file_path = Path::new(path_prefix)
                        .join(format!("{}.{}", texture_label, TEXTURE_FORMAT_NAME));
                    let file_bytes = fs::read(&file_path)
                        .with_context(|| format!("reading {:?}", &file_path))?;

                    let texture =
                        image::load_from_memory_with_format(file_bytes.as_ref(), TEXTURE_FORMAT)?
                            .into_rgba8();

                    Ok((texture_label, texture))
                })
                .collect::<Result<Vec<_>, anyhow::Error>>()?;

            let mut locations = textures
                .iter()
                .map(|(label, texture)| {
                    let tex_width = texture
                        .dimensions()
                        .0
                        .try_into()
                        .expect("texture too large");
                    let tex_height = texture
                        .dimensions()
                        .1
                        .try_into()
                        .expect("texture too large");

                    let idx_pos = packers.iter_mut().enumerate().find_map(|(idx, packer)| {
                        Some((idx, packer.pack(tex_width, tex_height, false)?))
                    });

                    let (idx, pos) = idx_pos
                        .or_else(|| {
                            let curr_size = packers[0].size().0 as u32;

                            if packers.len() != 1 || curr_size >= max_size {
                                return None;
                            }
                            let packer = packers.first_mut().unwrap();
                            let new_size = (packer.size().0 as u32 * 2)
                                .min(max_size)
                                .try_into()
                                .unwrap();
                            packer.resize(new_size, new_size);

                            Some((0, packer.pack(tex_width, tex_height, false)?))
                        })
                        .map_or_else(
                            || {
                                let curr_size = packers[0].size().0;

                                let mut packer = DensePacker::new(curr_size, curr_size);
                                let pos =
                                    packer.pack(tex_width, tex_height, false).ok_or_else(|| {
                                        anyhow::anyhow!(
                                            "texture \"{}\" is too large, maximum size is {}",
                                            label,
                                            curr_size
                                        )
                                    })?;

                                let idx = packers.len();
                                packers.push(packer);

                                Ok::<_, anyhow::Error>((idx, pos))
                            },
                            |idx_pos| Ok(idx_pos),
                        )?;

                    Ok::<_, anyhow::Error>(Location {
                        data_index: idx.try_into().unwrap(),
                        position: [pos.x as u32, pos.y as u32],
                        size: [pos.width as u32, pos.height as u32],
                        edge_correction: [0.0; 2],
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            for location in locations.iter_mut() {
                let size = packers[location.data_index as usize].size();

                location.edge_correction =
                    [size.0, size.1].map(|size| (EDGE_CORRECTION_PIXELS / size as f64) as f32);
            }

            let mut data = packers
                .iter()
                .map(|packer| {
                    let (size_x, size_y) = packer.size();
                    RgbaImage::new(size_x as u32, size_y as u32)
                })
                .collect::<Vec<_>>();

            for (texture_id, location) in locations.iter().enumerate() {
                let data = data.get_mut(location.data_index as usize).unwrap();
                data.sub_image(
                    location.position[0],
                    location.position[1],
                    location.size[0],
                    location.size[1],
                )
                .copy_from(&textures[texture_id].1, 0, 0)
                .expect("incorrect texture location calculation");
            }

            let label_map = LabelMap::from_list(&texture_list.list);

            Ok::<_, Error>((locations, Self { data, label_map }))
        })
        .await
        .unwrap()?;

        let atlas_size = [slf.data[0].dimensions().0, slf.data[0].dimensions().1];

        location_tc.load(atlas_size, locations);

        Ok(slf)
    }

    pub fn label_map(&self) -> LabelMap<Texture> {
        self.label_map.clone()
    }

    pub fn prepare_buffer(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture_format: &wgpu::TextureFormat,
    ) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
        let texture_size = self.data[0].dimensions();

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

        let texture_views = self
            .data
            .iter()
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
                        count: NonZeroU32::new(self.data.len() as u32),
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: NonZeroU32::new(self.data.len() as u32),
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
                        &self.data.iter().map(|_| &sampler).collect::<Vec<_>>(),
                    ),
                },
            ],
            layout: &texture_bind_group_layout,
            label: Some("texture_bind_group"),
        });

        (texture_bind_group_layout, texture_bind_group)
    }
}
