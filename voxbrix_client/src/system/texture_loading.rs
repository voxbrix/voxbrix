use crate::entity::texture::Texture;
use anyhow::{
    Context,
    Error,
};
use image::GenericImageView;
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

const TEXTURE_FORMAT_NAME: &str = "png";
const TEXTURE_FORMAT_SHADER: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Uint;
const TEXTURE_BYTES_PER_PIXEL: u32 = 4;

#[derive(Deserialize, Debug)]
struct TextureList {
    list: Vec<String>,
}

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
    ) -> Result<Self, Error> {
        let texture_list: TextureList = task::spawn_blocking(move || read_data_file(list_path))
            .await
            .unwrap()?;

        let label_map = LabelMap::from_list(&texture_list.list);

        let texture_views = {
            let device = device.clone();
            let queue = queue.clone();

            task::spawn_blocking(move || {
                texture_list
                    .list
                    .into_iter()
                    .map(|texture_label| {
                        let file_path = Path::new(path_prefix)
                            .join(format!("{}.{}", texture_label, TEXTURE_FORMAT_NAME));

                        let image_file = fs::read(&file_path)
                            .with_context(|| format!("reading texture file {:?}", &file_path))?;

                        let image = image::load_from_memory(&image_file)
                            .with_context(|| format!("reading image from {:?}", &file_path))?;

                        let layers = 1;

                        load_texture(&device, &queue, &image, &texture_label, layers)
                            .with_context(|| format!("loading texture {:?}", &file_path))
                    })
                    .collect::<Result<Vec<_>, anyhow::Error>>()
            })
        }
        .await
        .unwrap()?;

        let count: u32 = texture_views.len().try_into().expect("too many textures");

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: Some(
                    count
                        .try_into()
                        .with_context(|| format!("no textures provided in {:?}", &list_path))?,
                ),
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureViewArray(
                    &texture_views.iter().collect::<Vec<_>>(),
                ),
            }],
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
