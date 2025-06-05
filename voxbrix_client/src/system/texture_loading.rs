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
use image::ImageFormat;
use rect_packer::DensePacker;
use serde::Deserialize;
use std::{
    fs,
    path::Path,
};
use tokio::task;
use voxbrix_common::{
    read_data_file,
    LabelMap,
};

const TEXTURE_FORMAT: ImageFormat = ImageFormat::Png;
const TEXTURE_FORMAT_NAME: &str = "png";
const TEXTURE_FORMAT_SHADER: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Uint;
const TEXTURE_BYTES_PER_PIXEL: u32 = 4;
const MIN_TEXTURE_ATLAS_SIZE: u32 = 512;
const MAX_TEXTURE_ATLAS_SIZE: u32 = 8096;
const EDGE_CORRECTION_PIXELS: f64 = 0.001;

#[derive(Deserialize, Debug)]
struct TextureList {
    list: Vec<String>,
}

pub struct TextureLoadingSystem {
    label_map: LabelMap<Texture>,
}

impl TextureLoadingSystem {
    pub async fn load_data(
        device: &wgpu::Device,
        list_path: &'static str,
        path_prefix: &'static str,
        location_tc: &mut LocationTextureComponent,
    ) -> Result<Self, Error> {
        let max_size: i32 = device
            .limits()
            .max_texture_dimension_2d
            .min(MAX_TEXTURE_ATLAS_SIZE)
            .try_into()
            .unwrap();

        let texture_list: TextureList = read_data_file(list_path)?;

        let label_map = LabelMap::from_list(&texture_list.list);

        let mut packers = vec![DensePacker::new(
            MIN_TEXTURE_ATLAS_SIZE.try_into().unwrap(),
            MIN_TEXTURE_ATLAS_SIZE.try_into().unwrap(),
        )];

        let texture_dimensions = task::spawn_blocking(move || {
            texture_list
                .list
                .into_iter()
                .map(|texture_label| {
                    let file_path = Path::new(path_prefix)
                        .join(format!("{}.{}", texture_label, TEXTURE_FORMAT_NAME));

                    let dimensions = image::image_dimensions(&file_path)
                        .with_context(|| format!("reading dimensions of {:?}", &file_path))?;

                    Ok((texture_label, dimensions))
                })
                .collect::<Result<Vec<_>, anyhow::Error>>()
        })
        .await
        .unwrap()?;

        let mut locations = texture_dimensions
            .iter()
            .map(|(label, texture_dimensions)| {
                let tex_width = texture_dimensions.0.try_into().expect("texture too large");
                let tex_height = texture_dimensions.1.try_into().expect("texture too large");

                let idx_pos = packers.iter_mut().enumerate().find_map(|(idx, packer)| {
                    Some((idx, packer.pack(tex_width, tex_height, false)?))
                });

                let (idx, pos) = idx_pos
                    .or_else(|| {
                        let packer = packers.first_mut().expect("first packer must exist");

                        let mut curr_size = packer.size().0;

                        while curr_size < max_size {
                            curr_size = (packer.size().0 * 2).min(max_size);

                            packer.resize(curr_size, curr_size);

                            if let Some(packed) = packer.pack(tex_width, tex_height, false) {
                                return Some((0, packed));
                            }
                        }

                        None
                    })
                    .map_or_else(
                        || {
                            let curr_size =
                                packers.first().expect("first packer must exist").size().0;

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

        let atlas_size = [packers[0].size().0 as u32, packers[0].size().1 as u32];
        let atlas_layers = packers.len().try_into().unwrap();

        location_tc.load(atlas_size, atlas_layers, locations);

        Ok(Self { label_map })
    }

    pub fn label_map(&self) -> LabelMap<Texture> {
        self.label_map.clone()
    }

    pub async fn prepare_buffer(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path_prefix: &'static str,
        location_tc: &LocationTextureComponent,
    ) -> Result<(wgpu::BindGroupLayout, wgpu::BindGroup), Error> {
        let texture_descriptior = wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: location_tc.atlas_size()[0],
                height: location_tc.atlas_size()[1],
                depth_or_array_layers: location_tc.atlas_layers(),
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEXTURE_FORMAT_SHADER,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("texture"),
            view_formats: &[TEXTURE_FORMAT_SHADER],
        };

        let texture = device.create_texture(&texture_descriptior);

        for (texture_id, location) in location_tc.iter() {
            let texture_label = self.label_map.get_label(&texture_id).ok_or_else(|| {
                anyhow::anyhow!("texture not found, incorrect LocationTextureComponent provided")
            })?;

            let file_path =
                Path::new(path_prefix).join(format!("{}.{}", texture_label, TEXTURE_FORMAT_NAME));
            let texture_bytes = {
                let file_bytes = task::spawn_blocking(move || {
                    fs::read(&file_path).with_context(|| format!("reading {:?}", &file_path))
                })
                .await
                .unwrap()?;

                image::load_from_memory_with_format(file_bytes.as_ref(), TEXTURE_FORMAT)
                    .with_context(|| format!("incorrect format of texture {}", texture_label))?
                    .into_rgba8()
            };

            let dim_ctrl = texture_bytes.dimensions();

            if dim_ctrl.0 != location.size[0] || dim_ctrl.1 != location.size[1] {
                anyhow::bail!("dimensions of texture \"{:?}\" changed", texture_label);
            }

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: location.position[0],
                        y: location.position[1],
                        z: location.data_index,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &texture_bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(TEXTURE_BYTES_PER_PIXEL * location.size[0]),
                    rows_per_image: Some(location.size[1]),
                },
                wgpu::Extent3d {
                    width: location.size[0],
                    height: location.size[1],
                    depth_or_array_layers: 1,
                },
            );
            queue.submit([]);
        }

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                }],
            });

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_view),
            }],
            layout: &texture_bind_group_layout,
            label: Some("texture_bind_group"),
        });

        Ok((texture_bind_group_layout, texture_bind_group))
    }
}
