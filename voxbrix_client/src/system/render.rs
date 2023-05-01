use crate::{
    camera::{
        Camera,
        Projection,
    },
    component::block_class::{
        culling::{
            Culling,
            CullingBlockClassComponent,
        },
        model::{
            Cube,
            CullMask,
            CullMaskSides,
            ModelBlockClassComponent,
        },
    },
    entity::vertex::Vertex,
    RenderHandle,
};
use anyhow::Result;
use arrayvec::ArrayVec;
use rayon::prelude::*;
use std::{
    collections::BTreeMap,
    iter,
    num::NonZeroU64,
};
use voxbrix_common::{
    component::{
        actor::{
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
        },
        block::{
            class::ClassBlockComponent,
            sky_light::{
                SkyLight,
                SkyLightBlockComponent,
            },
            BlocksVec,
        },
    },
    entity::{
        actor::Actor,
        block::{
            Block,
            Neighbor,
            BLOCKS_IN_CHUNK,
        },
        block_class::BlockClass,
        chunk::Chunk,
    },
    math::{
        Mat4,
        Vec3,
    },
};
use wgpu::util::{
    BufferInitDescriptor,
    DeviceExt,
};
use winit::dpi::PhysicalSize;

const INDEX_FORMAT: wgpu::IndexFormat = wgpu::IndexFormat::Uint32;
const INDEX_SIZE: usize = std::mem::size_of::<u32>();
const VERTEX_SIZE: usize = Vertex::size() as usize;
const VERTEX_BUFFER_CAPACITY: usize = BLOCKS_IN_CHUNK * 6 /*sides*/ * 4 /*vertices*/;
const INDEX_BUFFER_CAPACITY: usize = BLOCKS_IN_CHUNK * 6 /*sides*/ * 2 /*polygons*/ * 3 /*vertices*/;

fn neighbors_to_cull_mask(
    neighbors: &[Neighbor; 6],
    this_chunk: &BlocksVec<BlockClass>,
    neighbor_chunks: &[Option<&BlocksVec<BlockClass>>; 6],
    culling_bcc: &CullingBlockClassComponent,
) -> CullMask {
    let mut cull_mask = CullMask::all();
    for (i, (neighbor, neighbor_chunk)) in neighbors.iter().zip(neighbor_chunks.iter()).enumerate()
    {
        let side = CullMaskSides::from_index(i).expect("correct cull mask side index");

        match neighbor {
            Neighbor::ThisChunk(n) => {
                let class = this_chunk.get(*n);
                let culling = culling_bcc.get(*class);
                match culling {
                    Some(Culling::Full) => {
                        cull_mask.unset(side);
                    },
                    None => {},
                }
            },
            Neighbor::OtherChunk(n) => {
                if let Some(chunk) = neighbor_chunk {
                    let class = chunk.get(*n);
                    let culling = culling_bcc.get(*class);
                    match culling {
                        Some(Culling::Full) => {
                            cull_mask.unset(side);
                        },
                        None => {},
                    }
                } else {
                    cull_mask.unset(side);
                }
            },
        }
    }

    cull_mask
}

fn build_depth_texture_view(
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
        view_formats: &[wgpu::TextureFormat::Depth32Float],
    };
    let texture = device.create_texture(&desc);

    texture.create_view(&wgpu::TextureViewDescriptor::default())
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
        position_ac: &PositionActorComponent,
        orientation_ac: &OrientationActorComponent,
        projection: &Projection,
    ) {
        let pos = position_ac.get(&camera.actor).unwrap();
        self.chunk = pos.chunk.position.into();
        self.view_position = pos.offset.to_homogeneous();
        self.view_projection = match camera.calc_matrix(position_ac, orientation_ac) {
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
    chunk_info: &mut [ChunkInfo<'a>],
    mut vertex_buffer: &'a mut [u8],
    mut index_buffer: &'a mut [u8],
) {
    for chunk in chunk_info.iter_mut() {
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

pub struct RenderSystemDescriptor<'a> {
    pub render_handle: &'static RenderHandle,
    pub surface_size: PhysicalSize<u32>,
    pub player_actor: Actor,
    pub position_ac: &'a PositionActorComponent,
    pub orientation_ac: &'a OrientationActorComponent,
    pub block_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub block_texture_bind_group: wgpu::BindGroup,
}

impl<'a> RenderSystemDescriptor<'a> {
    pub async fn build(self) -> RenderSystem {
        let Self {
            render_handle,
            surface_size,
            player_actor,
            position_ac,
            orientation_ac,
            block_texture_bind_group_layout,
            block_texture_bind_group,
        } = self;

        let capabilities = render_handle
            .surface
            .get_capabilities(&render_handle.adapter);

        let format = capabilities
            .formats
            .into_iter()
            .find(|format| format == &wgpu::TextureFormat::Rgba8UnormSrgb)
            .expect("texture format found");

        let present_mode = capabilities
            .present_modes
            .into_iter()
            .find(|pm| *pm == wgpu::PresentMode::Mailbox)
            .unwrap_or(wgpu::PresentMode::Immediate);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: surface_size.width,
            height: surface_size.height,
            // Fifo makes SurfaceTexture::present() block
            // which is bad for current rendering implementation
            present_mode,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![format],
        };

        render_handle
            .surface
            .configure(&render_handle.device, &config);

        let camera = Camera::new(player_actor);
        let projection = Projection::new(
            config.width,
            config.height,
            std::f32::consts::FRAC_PI_4,
            0.1,
            100.0,
        );

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_projection(&camera, position_ac, orientation_ac, &projection);

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
                            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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
                        depth_compare: wgpu::CompareFunction::Less,
                        stencil: wgpu::StencilState::default(),
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

        let depth_texture_view = build_depth_texture_view(&render_handle.device, &config);

        // Target block hightlighting
        let target_highlight_vertex_buffer =
            render_handle.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Vertex Buffer"),
                size: 4 * Vertex::size(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

        let target_highlight_index_buffer =
            render_handle.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Index Buffer"),
                size: 6 * 4,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

        RenderSystem {
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
            render_target_highlight: false,
            target_highlight_vertices: Vec::with_capacity(4),
            target_highlight_indices: Vec::with_capacity(6),
            target_highlight_vertex_buffer,
            target_highlight_index_buffer,
        }
    }
}

pub struct RenderSystem {
    render_handle: &'static RenderHandle,
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
    render_target_highlight: bool,
    target_highlight_vertices: Vec<Vertex>,
    target_highlight_indices: Vec<u32>,
    target_highlight_vertex_buffer: wgpu::Buffer,
    target_highlight_index_buffer: wgpu::Buffer,
}

impl RenderSystem {
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.render_handle
                .surface
                .configure(&self.render_handle.device, &self.config);
            self.depth_texture_view =
                build_depth_texture_view(&self.render_handle.device, &self.config);
            self.projection.resize(new_size.width, new_size.height);
        }
    }

    pub fn update(
        &mut self,
        position_ac: &PositionActorComponent,
        orientation_ac: &OrientationActorComponent,
    ) {
        self.camera_uniform.update_view_projection(
            &self.camera,
            position_ac,
            orientation_ac,
            &self.projection,
        );
        self.render_handle.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );
    }

    fn build_chunk_buffer_shard(
        chunk: &Chunk,
        class_bc: &ClassBlockComponent,
        model_bcc: &ModelBlockClassComponent,
        culling_bcc: &CullingBlockClassComponent,
        sky_light_bc: &SkyLightBlockComponent,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertex_buffer = Vec::with_capacity(VERTEX_BUFFER_CAPACITY);
        let mut index_buffer = Vec::with_capacity(INDEX_BUFFER_CAPACITY);

        let neighbor_chunk_ids = [
            Vec3::new(-1, 0, 0),
            Vec3::new(1, 0, 0),
            Vec3::new(0, -1, 0),
            Vec3::new(0, 1, 0),
            Vec3::new(0, 0, -1),
            Vec3::new(0, 0, 1),
        ]
        .map(|offset| chunk.offset(offset));

        let this_chunk_class = class_bc.get_chunk(chunk).unwrap();
        let this_chunk_light = sky_light_bc.get_chunk(chunk).unwrap();

        let neighbor_chunk_class = neighbor_chunk_ids.map(|chunk| {
            let block_classes = class_bc.get_chunk(&chunk?)?;

            Some(block_classes)
        });

        let neighbor_chunk_light = neighbor_chunk_ids.map(|chunk| {
            let block_light = sky_light_bc.get_chunk(&chunk?)?;

            Some(block_light)
        });

        for (block, block_coords, block_class) in this_chunk_class.iter_with_coords() {
            if let Some(model) = model_bcc.get(*block_class) {
                let neighbors = block.neighbors_in_coords(block_coords);

                let cull_mask = neighbors_to_cull_mask(
                    &neighbors,
                    this_chunk_class,
                    &neighbor_chunk_class,
                    culling_bcc,
                );

                let sky_light_levels = neighbors
                    .iter()
                    .zip(neighbor_chunk_light)
                    .map(|(neighbor, neighbor_chunk_light)| {
                        Some(match neighbor {
                            Neighbor::ThisChunk(block) => *this_chunk_light.get(*block),
                            Neighbor::OtherChunk(block) => *neighbor_chunk_light?.get(*block),
                        })
                    })
                    .map(|light| light.unwrap_or(SkyLight::MIN).value())
                    .collect::<ArrayVec<_, 6>>()
                    .into_inner()
                    .unwrap_or_else(|_| unreachable!());

                model.to_vertices(
                    &mut vertex_buffer,
                    &mut index_buffer,
                    chunk,
                    block_coords,
                    cull_mask,
                    sky_light_levels,
                );
            }
        }

        (vertex_buffer, index_buffer)
    }

    pub fn build_chunk(
        &mut self,
        chunk: &Chunk,
        class_bc: &ClassBlockComponent,
        model_bcc: &ModelBlockClassComponent,
        culling_bcc: &CullingBlockClassComponent,
        sky_light_bc: &SkyLightBlockComponent,
    ) {
        if class_bc.get_chunk(chunk).is_none() {
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
        .filter_map(|offset| {
            let chunk = chunk.offset(offset)?;
            class_bc.get_chunk(&chunk)?;

            Some((
                chunk,
                Self::build_chunk_buffer_shard(
                    &chunk,
                    class_bc,
                    model_bcc,
                    culling_bcc,
                    sky_light_bc,
                ),
            ))
        });

        self.chunk_buffer_shards.par_extend(par_iter);

        self.update_chunk_buffer = true;
    }

    pub fn build_target_highlight(&mut self, target: Option<(Chunk, Block, usize)>) {
        if let Some((chunk, block, side)) = target {
            self.target_highlight_vertices.clear();
            self.target_highlight_indices.clear();

            Cube::add_side_highlighting(
                chunk.position,
                &mut self.target_highlight_vertices,
                &mut self.target_highlight_indices,
                block.to_coords(),
                side,
            );

            self.render_handle.queue.write_buffer(
                &self.target_highlight_vertex_buffer,
                0,
                bytemuck::cast_slice(&self.target_highlight_vertices),
            );

            self.render_handle.queue.write_buffer(
                &self.target_highlight_index_buffer,
                0,
                bytemuck::cast_slice(&self.target_highlight_indices),
            );

            self.render_target_highlight = true;
        } else {
            self.render_target_highlight = false;
        }
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
                        size: index_byte_size,
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

                slice_buffers(
                    &mut chunk_info,
                    vertex_writer.as_mut().unwrap(),
                    index_writer.as_mut().unwrap(),
                );

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
                        .copy_from_slice(bytemuck::cast_slice(vertex_vec));
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
                        r: 0.5,
                        g: 0.6,
                        b: 0.7,
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

        if self.render_target_highlight {
            render_pass.set_vertex_buffer(0, self.target_highlight_vertex_buffer.slice(..));
            render_pass
                .set_index_buffer(self.target_highlight_index_buffer.slice(..), INDEX_FORMAT);
            render_pass.draw_indexed(0 .. 6, 0, 0 .. 1);
        }

        drop(render_pass);

        self.render_handle
            .queue
            .submit(iter::once(encoder.finish()));

        output.present();

        Ok(())
    }
}
