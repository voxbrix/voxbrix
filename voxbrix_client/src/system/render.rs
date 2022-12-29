// use image::GenericImageView;
use crate::{
    camera::{
        Camera,
        Projection,
    },
    component::{
        actor::{
            orientation::OrientationActorComponent,
            position::GlobalPositionActorComponent,
        },
        block::{
            class::ClassBlockComponent,
            Blocks,
        },
        block_class::model::{
            CullMask,
            CullMaskSides,
            ModelBlockClassComponent,
        },
    },
    entity::{
        actor::Actor,
        block::{
            Neighbor,
            BLOCKS_IN_CHUNK,
        },
        block_class::BlockClass,
        chunk::Chunk,
        vertex::Vertex,
    },
    RenderHandle,
};
use anyhow::Result;
use async_fs::File;
use futures_lite::io::AsyncReadExt;
use image::{
    ImageFormat,
    RgbaImage,
};
use rayon::prelude::*;
use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
    hash::Hash,
    iter,
    num::{
        NonZeroU32,
        NonZeroU64,
    },
    ops::Deref,
    path::{
        Path,
        PathBuf,
    },
};
use voxbrix_common::math::{
    Mat4,
    Vec3,
};
use wgpu::util::{
    BufferInitDescriptor,
    DeviceExt,
};
use winit::dpi::PhysicalSize;

const BLOCK_TEXTURE_FORMAT: ImageFormat = ImageFormat::Png;
const INDEX_FORMAT: wgpu::IndexFormat = wgpu::IndexFormat::Uint32;
const INDEX_SIZE: usize = std::mem::size_of::<u32>();
const VERTEX_SIZE: usize = Vertex::size() as usize;
const VERTEX_BUFFER_CAPACITY: usize = BLOCKS_IN_CHUNK * 6 /*sides*/ * 4 /*vertices*/;
const INDEX_BUFFER_CAPACITY: usize = BLOCKS_IN_CHUNK * 6 /*sides*/ * 2 /*polygons*/ * 3 /*vertices*/;

async fn load_block_textures<'a, T>(
    base_path: PathBuf,
    file_names: &'a [T],
) -> Result<(Vec<RgbaImage>, HashMap<&'a T, usize>)>
where
    T: AsRef<Path> + Hash + Eq,
{
    let mut textures = Vec::with_capacity(file_names.len());
    let mut texture_names = HashMap::with_capacity(file_names.len());
    let mut buf = Vec::with_capacity(1024);

    for (index, file_name) in file_names.into_iter().enumerate() {
        let mut file_path = base_path.clone();
        file_path.push(&file_name);

        let mut file = File::open(file_path).await?;

        file.read_to_end(&mut buf).await?;

        let bytes_rgba =
            image::load_from_memory_with_format(buf.as_slice(), BLOCK_TEXTURE_FORMAT)?.to_rgba8();

        textures.push(bytes_rgba);
        texture_names.insert(file_name, index);

        buf.clear();
    }

    Ok((textures, texture_names))
}

fn neighbors_to_cull_mask<'a, T>(
    neighbors: &[Neighbor; 6],
    this_chunk: &Blocks<BlockClass>,
    neighbor_chunks: &[Option<T>; 6],
    model_bcc: &ModelBlockClassComponent,
) -> CullMask
where
    T: Deref<Target = &'a Blocks<BlockClass>>,
{
    let mut cull_mask = CullMask::all();
    for (i, (neighbor, neighbor_chunk)) in neighbors.iter().zip(neighbor_chunks.iter()).enumerate()
    {
        let side = CullMaskSides::from_index(i).expect("correct cull mask side index");

        match neighbor {
            Neighbor::ThisChunk(n) => {
                let model = this_chunk.get(*n).and_then(|bc| model_bcc.get(*bc));
                if model.is_some() {
                    cull_mask.unset(side);
                }
            },
            Neighbor::OtherChunk(n) => {
                if let Some(chunk) = neighbor_chunk {
                    let model = chunk.get(*n).and_then(|bc| model_bcc.get(*bc));
                    if model.is_some() {
                        cull_mask.unset(side);
                    }
                } else {
                    cull_mask.unset(side);
                }
            },
        }
    }

    cull_mask
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    chunk: [i32; 3],
    _padding: u32,
    view_position: [f32; 4],
    view_projection: [[f32; 4]; 4],
}

impl CameraUniform {
    fn new() -> Self {
        Self {
            chunk: [0; 3],
            _padding: 0,
            view_position: [0.0; 4],
            view_projection: Mat4::identity().into(),
        }
    }

    fn update_view_projection(
        &mut self,
        camera: &Camera,
        gpac: &GlobalPositionActorComponent,
        oac: &OrientationActorComponent,
        projection: &Projection,
    ) {
        let pos = gpac.get(&camera.actor).unwrap();
        self.chunk = pos.chunk.position.into();
        self.view_position = pos.offset.to_homogeneous();
        self.view_projection = match camera.calc_matrix(gpac, oac) {
            Some(camera_matrix) => (projection.calc_matrix() * camera_matrix).into(),
            None => self.view_projection,
        };
    }
}

struct ChunkInfo<'a> {
    chunk_shard: &'a (Vec<Vertex>, Vec<u32>),
    vertex_offset: usize,
    vertex_length: usize,
    index_length: usize,
    vertex_buffer: Option<&'a mut [u8]>,
    index_buffer: Option<&'a mut [u8]>,
}

fn slice_buffers<'a>(
    chunk_info: &mut Vec<ChunkInfo<'a>>,
    mut vertex_buffer: &'a mut [u8],
    mut index_buffer: &'a mut [u8],
) {
    for chunk in chunk_info.into_iter() {
        let (vertex_buffer_shard, residue) =
            vertex_buffer.split_at_mut(chunk.vertex_length * VERTEX_SIZE);
        vertex_buffer = residue;
        chunk.vertex_buffer = Some(vertex_buffer_shard);

        let (index_buffer_shard, residue) =
            index_buffer.split_at_mut(chunk.index_length * INDEX_SIZE);
        index_buffer = residue;
        chunk.index_buffer = Some(index_buffer_shard);
    }
}

pub struct RenderSystem<'a> {
    render_handle: &'a RenderHandle,
    config: wgpu::SurfaceConfiguration,
    pub size: PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    chunk_buffer_shards: BTreeMap<Chunk, (Vec<Vertex>, Vec<u32>)>,
    update_chunk_buffer: bool,
    prepared_vertex_buffer: wgpu::Buffer,
    prepared_index_buffer: wgpu::Buffer,
    num_indices: u32,
    block_texture_bind_group: wgpu::BindGroup,
    depth_texture_view: wgpu::TextureView,
    camera: Camera,
    camera_uniform: CameraUniform,
    projection: Projection,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
}

impl<'a> RenderSystem<'a> {
    pub async fn load_block_textures(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        files: &[&str],
    ) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
        let (block_texture_bytes, _block_texture_names) =
            load_block_textures("./assets".into(), files).await.unwrap();

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
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("block_texture"),
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
                label: Some("bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: NonZeroU32::new(2),
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: NonZeroU32::new(2),
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
                    resource: wgpu::BindingResource::SamplerArray(&[&sampler, &sampler]),
                },
            ],
            layout: &texture_bind_group_layout,
            label: Some("bind group"),
        });

        (texture_bind_group_layout, texture_bind_group)
    }

    pub fn build_depth_texture_view(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::TextureView {
        let size = wgpu::Extent3d {
            // 2.
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        };

        let desc = wgpu::TextureDescriptor {
            label: Some("depth_texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT // 3.
                | wgpu::TextureUsages::TEXTURE_BINDING,
        };
        let texture = device.create_texture(&desc);

        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    // Creating some of the wgpu types requires async code
    pub async fn new(
        render_handle: &'a RenderHandle,
        surface_size: PhysicalSize<u32>,
        gpac: &GlobalPositionActorComponent,
        oac: &OrientationActorComponent,
    ) -> RenderSystem<'a> {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: render_handle
                .surface
                .get_supported_formats(&render_handle.adapter)[0],
            width: surface_size.width,
            height: surface_size.height,
            // Fifo makes SurfaceTexture::present() block
            present_mode: wgpu::PresentMode::Mailbox,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
        };

        render_handle
            .surface
            .configure(&render_handle.device, &config);

        let (block_texture_bind_group_layout, block_texture_bind_group) =
            Self::load_block_textures(
                &render_handle.device,
                &render_handle.queue,
                &["grass.png", "dirt.png"],
            )
            .await;

        let camera = Camera::new(Actor(0));
        let projection = Projection::new(
            config.width,
            config.height,
            std::f32::consts::FRAC_PI_4,
            0.1,
            100.0,
        );

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_projection(&camera, gpac, oac, &projection);

        let camera_buffer = render_handle
            .device
            .create_buffer_init(&BufferInitDescriptor {
                label: Some("Camera Buffer"),
                contents: bytemuck::cast_slice(&[camera_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let camera_bind_group_layout =
            render_handle
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                    label: Some("camera_bind_group_layout"),
                });

        let camera_bind_group =
            render_handle
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &camera_bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: camera_buffer.as_entire_binding(),
                    }],
                    label: Some("camera_bind_group"),
                });

        let shaders = render_handle
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Shaders"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders.wgsl").into()),
            });

        let render_pipeline_layout =
            render_handle
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Render Pipeline Layout"),
                    bind_group_layouts: &[
                        &block_texture_bind_group_layout,
                        &camera_bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                });

        let render_pipeline =
            render_handle
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Render Pipeline"),
                    layout: Some(&render_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shaders,
                        entry_point: "vs_main",
                        buffers: &[Vertex::desc()],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shaders,
                        entry_point: "fs_main",
                        targets: &[Some(wgpu::ColorTargetState {
                            format: config.format,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Cw,
                        cull_mode: Some(wgpu::Face::Back),
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: wgpu::TextureFormat::Depth32Float,
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::Less, // 1.
                        stencil: wgpu::StencilState::default(),     // 2.
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview: None,
                });

        let vertex_buffer = render_handle
            .device
            .create_buffer_init(&BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice::<Vertex, u8>(&[]),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let index_buffer = render_handle
            .device
            .create_buffer_init(&BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice::<Vertex, u8>(&[]),
                usage: wgpu::BufferUsages::INDEX,
            });

        let depth_texture_view = Self::build_depth_texture_view(&render_handle.device, &config);

        Self {
            render_handle,
            config,
            size: surface_size,
            render_pipeline,
            chunk_buffer_shards: BTreeMap::new(),
            update_chunk_buffer: false,
            prepared_vertex_buffer: vertex_buffer,
            prepared_index_buffer: index_buffer,
            num_indices: 0,
            block_texture_bind_group,
            depth_texture_view,
            camera,
            projection,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
        }
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.render_handle
                .surface
                .configure(&self.render_handle.device, &self.config);
            self.depth_texture_view =
                Self::build_depth_texture_view(&self.render_handle.device, &self.config);
            self.projection.resize(new_size.width, new_size.height);
        }
    }

    pub fn update(&mut self, gpac: &GlobalPositionActorComponent, oac: &OrientationActorComponent) {
        self.camera_uniform
            .update_view_projection(&self.camera, gpac, oac, &self.projection);
        self.render_handle.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );
    }

    fn build_chunk_buffer_shard(
        chunk: &Chunk,
        cbc: &ClassBlockComponent,
        mbcc: &ModelBlockClassComponent,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertex_buffer = Vec::with_capacity(VERTEX_BUFFER_CAPACITY);
        let mut index_buffer = Vec::with_capacity(INDEX_BUFFER_CAPACITY);

        let blocks = cbc.get_chunk(chunk).unwrap();

        let chunk_x_minus = Some(chunk).and_then(|cz| {
            let mut chunk = cz.clone();
            chunk.position[0] = cz.position[0].checked_sub(1)?;
            cbc.get_chunk(&chunk)
        });
        let chunk_x_plus = Some(chunk).and_then(|cz| {
            let mut chunk = cz.clone();
            chunk.position[0] = cz.position[0].checked_add(1)?;
            cbc.get_chunk(&chunk)
        });

        let chunk_y_minus = Some(chunk).and_then(|cz| {
            let mut chunk = cz.clone();
            chunk.position[1] = cz.position[1].checked_sub(1)?;
            cbc.get_chunk(&chunk)
        });
        let chunk_y_plus = Some(chunk).and_then(|cz| {
            let mut chunk = cz.clone();
            chunk.position[1] = cz.position[1].checked_add(1)?;
            cbc.get_chunk(&chunk)
        });

        let chunk_z_minus = Some(chunk).and_then(|cz| {
            let mut chunk = cz.clone();
            chunk.position[2] = cz.position[2].checked_sub(1)?;
            cbc.get_chunk(&chunk)
        });
        let chunk_z_plus = Some(chunk).and_then(|cz| {
            let mut chunk = cz.clone();
            chunk.position[2] = cz.position[2].checked_add(1)?;
            cbc.get_chunk(&chunk)
        });

        for (block, block_coords, block_class) in blocks.iter_with_coords() {
            if let Some(model) = mbcc.get(*block_class) {
                let cull_mask = neighbors_to_cull_mask(
                    &block.neighbors_in_coords(block_coords),
                    &blocks,
                    &[
                        chunk_x_minus.as_ref(),
                        chunk_x_plus.as_ref(),
                        chunk_y_minus.as_ref(),
                        chunk_y_plus.as_ref(),
                        chunk_z_minus.as_ref(),
                        chunk_z_plus.as_ref(),
                    ],
                    &mbcc,
                );
                model.to_vertices(
                    &mut vertex_buffer,
                    &mut index_buffer,
                    &chunk,
                    block_coords,
                    cull_mask,
                );
            }
        }

        (vertex_buffer, index_buffer)
    }

    pub fn build_chunk(
        &mut self,
        chunk: &Chunk,
        cbc: &ClassBlockComponent,
        mbcc: &ModelBlockClassComponent,
    ) {
        if cbc.get_chunk(chunk).is_none() {
            self.chunk_buffer_shards.remove(chunk);
        }

        let par_iter = [
            Vec3::new(0, 0, 0),
            Vec3::new(-1, 0, 0),
            Vec3::new(1, 0, 0),
            Vec3::new(0, -1, 0),
            Vec3::new(0, 1, 0),
            Vec3::new(0, 0, -1),
            Vec3::new(0, 0, 1),
        ]
        .into_par_iter()
        .filter_map(|shift| {
            let mut chunk = *chunk;

            chunk.position = chunk.position + shift;
            cbc.get_chunk(&chunk)?;

            Some((chunk, Self::build_chunk_buffer_shard(&chunk, cbc, mbcc)))
        });

        self.chunk_buffer_shards.par_extend(par_iter);

        self.update_chunk_buffer = true;
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        if self.update_chunk_buffer {
            let mut chunk_info = Vec::with_capacity(self.chunk_buffer_shards.len());

            let (vertex_size, index_size) =
                self.chunk_buffer_shards
                    .values()
                    .fold((0, 0), |(vbl, ibl), chunk_shard| {
                        let (ref vb, ref ib) = chunk_shard;
                        chunk_info.push(ChunkInfo {
                            chunk_shard,
                            vertex_offset: vbl,
                            vertex_length: vb.len(),
                            index_length: ib.len(),
                            vertex_buffer: None,
                            index_buffer: None,
                        });
                        (vbl + vb.len(), ibl + ib.len())
                    });

            let vertex_byte_size = (vertex_size * VERTEX_SIZE) as u64;
            let index_byte_size = (index_size * INDEX_SIZE) as u64;

            self.prepared_vertex_buffer =
                self.render_handle
                    .device
                    .create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Vertex Buffer"),
                        size: vertex_byte_size,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });

            self.prepared_index_buffer =
                self.render_handle
                    .device
                    .create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Index Buffer"),
                        size: index_byte_size as u64,
                        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });

            if vertex_size != 0 && index_size != 0 {
                let mut vertex_writer = self.render_handle.queue.write_buffer_with(
                    &self.prepared_vertex_buffer,
                    0,
                    NonZeroU64::new(vertex_byte_size).unwrap(),
                );

                let mut index_writer = self.render_handle.queue.write_buffer_with(
                    &self.prepared_index_buffer,
                    0,
                    NonZeroU64::new(index_byte_size).unwrap(),
                );

                // Change to .as_mut() if https://github.com/gfx-rs/wgpu/pull/3336
                slice_buffers(&mut chunk_info, &mut vertex_writer, &mut index_writer);

                chunk_info.par_iter_mut().for_each(|chunk| {
                    let (vertex_vec, index_vec) = chunk.chunk_shard;

                    let mut index_cursor = 0;
                    for index in index_vec.iter() {
                        chunk.index_buffer.as_mut().unwrap()
                            [index_cursor .. index_cursor + INDEX_SIZE]
                            .copy_from_slice(bytemuck::bytes_of(
                                &(index + chunk.vertex_offset as u32),
                            ));
                        index_cursor += INDEX_SIZE;
                    }

                    chunk
                        .vertex_buffer
                        .as_mut()
                        .unwrap()
                        .copy_from_slice(bytemuck::cast_slice(&vertex_vec));
                });
            }

            self.num_indices = index_size as u32;
            self.update_chunk_buffer = false;
        }

        let output = self.render_handle.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            self.render_handle
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.0,
                    }),
                    store: true,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_texture_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.block_texture_bind_group, &[]);
        render_pass.set_bind_group(1, &self.camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.prepared_vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.prepared_index_buffer.slice(..), INDEX_FORMAT);
        render_pass.draw_indexed(0 .. self.num_indices, 0, 0 .. 1);

        drop(render_pass);

        self.render_handle
            .queue
            .submit(iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
