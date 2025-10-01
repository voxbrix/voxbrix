use std::time::Duration;
use voxbrix_common::{
    entity::block::BLOCKS_IN_CHUNK_EDGE_F32,
    math::{
        Directions,
        Mat4F32,
        Vec3F32,
        Vec3I32,
    },
};
use wgpu::util::{
    BufferInitDescriptor,
    DeviceExt,
};

const CAMERA_NEAR: f32 = 0.01;

#[derive(Debug)]
pub struct CameraParameters {
    pub chunk: Vec3I32,
    pub offset: Vec3F32,
    pub view_direction: Vec3F32,
    pub aspect: f32,
    pub fovy: f32,
}

impl CameraParameters {
    fn calc_uniform(&self, animation_timer: u32) -> CameraUniform {
        let look_to = Mat4F32::look_to_lh(self.offset, self.view_direction, Vec3F32::UP);

        let perspective = Mat4F32::perspective_infinite_lh(self.fovy, self.aspect, CAMERA_NEAR);

        CameraUniform {
            chunk: self.chunk.into(),
            animation_timer,
            // offset converted to homogeneous
            view_position: [self.offset[0], self.offset[1], self.offset[2], 1.0],
            view_projection: (perspective * look_to).to_cols_array(),
        }
    }

    fn max_visible_angle(&self) -> f32 {
        ((self.aspect.powi(2) + 1.0).sqrt() * (self.fovy / 2.0).tan()).atan()
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    chunk: [i32; 3],
    animation_timer: u32,
    view_position: [f32; 4],
    view_projection: [f32; 16],
}

#[derive(Debug)]
pub struct Camera {
    parameters: CameraParameters,
    // Maximum angle from view direction that is still visible on the screen.
    // Half of the diagonal FOV.
    max_visible_angle: f32,
    buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    animation_timer: u32,
}

impl Camera {
    pub fn new(device: &wgpu::Device, parameters: CameraParameters) -> Self {
        let uniform = parameters.calc_uniform(0);

        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
            label: Some("Camera Bind Group Layout"),
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
            label: Some("Camera Bind Group"),
        });

        let max_visible_angle = parameters.max_visible_angle();

        Self {
            parameters,
            max_visible_angle,
            buffer,
            bind_group_layout,
            bind_group,
            animation_timer: 0,
        }
    }

    pub fn is_object_visible(&self, chunk: Vec3I32, offset: Vec3F32, radius: f32) -> bool {
        let vec_to_object = Vec3F32::from_array([0, 1, 2].map(|i| {
            let chunk_diff = chunk[i] - self.parameters.chunk[i];
            chunk_diff as f32 * BLOCKS_IN_CHUNK_EDGE_F32 + (offset[i] - self.parameters.offset[i])
        }));

        let object_dist = vec_to_object.length();

        if radius > object_dist {
            // Camera is within the object radius:
            return true;
        }

        let view_direction = self.parameters.view_direction;

        let object_angle =
            (vec_to_object.dot(view_direction) / (object_dist * view_direction.length())).acos();

        let max_visible_object_angle = (radius / object_dist).asin() + self.max_visible_angle;

        object_angle < max_visible_object_angle
    }

    /// Make sure height != 0
    pub fn resize(&mut self, width: u32, height: u32) {
        self.parameters.aspect = (width as f32) / (height as f32);
    }

    pub fn update(
        &mut self,
        chunk: Vec3I32,
        offset: Vec3F32,
        view_direction: Vec3F32,
        dt: Duration,
    ) {
        self.animation_timer = self.animation_timer.wrapping_add(dt.as_millis() as u32);
        self.parameters.chunk = chunk;
        self.parameters.offset = offset;
        self.parameters.view_direction = view_direction;
    }

    pub fn update_buffers(&mut self, queue: &wgpu::Queue) {
        self.max_visible_angle = self.parameters.max_visible_angle();
        let uniform = self.parameters.calc_uniform(self.animation_timer);
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    pub fn get_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn get_bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}
