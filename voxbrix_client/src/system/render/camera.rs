use voxbrix_common::{
    component::actor::{
        orientation::OrientationActorComponent,
        position::PositionActorComponent,
    },
    entity::actor::Actor,
    math::{
        Mat4,
        Vec3,
    },
};
use wgpu::util::{
    BufferInitDescriptor,
    DeviceExt,
};

const UP_VECTOR: Vec3<f32> = Vec3::new(0.0, 0.0, 1.0);

#[derive(Debug)]
enum CameraError {
    InvalidActor,
    InvalidCameraParameters,
}

#[derive(Debug)]
pub struct CameraParameters {
    pub aspect: f32,
    pub fovy: f32,
    pub near: f32,
    pub far: f32,
}

impl CameraParameters {
    pub fn calc_perspective(&self) -> Mat4<f32> {
        Mat4::perspective_lh(self.aspect, self.fovy, self.near, self.far)
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    chunk: [i32; 3],
    _padding: u32,
    view_position: [f32; 4],
    view_projection: [[f32; 4]; 4],
}

fn calc_uniform(
    actor: &Actor,
    parameters: &CameraParameters,
    position_ac: &PositionActorComponent,
    orientation_ac: &OrientationActorComponent,
) -> Result<CameraUniform, CameraError> {
    let position = position_ac.get(actor).ok_or(CameraError::InvalidActor)?;
    let orientation = orientation_ac.get(actor).ok_or(CameraError::InvalidActor)?;

    let look_to = Mat4::look_to_lh(position.offset, orientation.forward(), UP_VECTOR)
        .ok_or(CameraError::InvalidCameraParameters)?;

    Ok(CameraUniform {
        chunk: position.chunk.position.into(),
        _padding: 0,
        view_position: position.offset.to_homogeneous(),
        view_projection: (parameters.calc_perspective() * look_to).into(),
    })
}

#[derive(Debug)]
pub struct Camera {
    pub actor: Actor,
    pub parameters: CameraParameters,
    buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

impl Camera {
    pub fn new(
        device: &wgpu::Device,
        actor: Actor,
        parameters: CameraParameters,
        position_ac: &PositionActorComponent,
        orientation_ac: &OrientationActorComponent,
    ) -> Self {
        let uniform = calc_uniform(&actor, &parameters, position_ac, orientation_ac).unwrap();

        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        Self {
            actor,
            parameters,
            buffer,
            bind_group_layout,
            bind_group,
        }
    }

    /// Make sure height != 0
    pub fn resize(&mut self, width: u32, height: u32) {
        self.parameters.aspect = (width as f32) / (height as f32);
    }

    pub fn update(
        &self,
        queue: &wgpu::Queue,
        position_ac: &PositionActorComponent,
        orientation_ac: &OrientationActorComponent,
    ) {
        if let Ok(uniform) =
            calc_uniform(&self.actor, &self.parameters, position_ac, orientation_ac)
        {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[uniform]));
        }
    }

    pub fn get_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn get_bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}
