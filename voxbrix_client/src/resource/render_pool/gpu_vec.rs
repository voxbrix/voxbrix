use std::num::NonZeroU64;

pub struct GpuVec {
    buffer: wgpu::Buffer,
    length: wgpu::BufferAddress,
}

impl GpuVec {
    pub fn new(device: &wgpu::Device, mut usage: wgpu::BufferUsages) -> Self {
        const INIT_CAPACITY: u64 = 100 * 1024;

        usage.insert(wgpu::BufferUsages::COPY_DST);

        Self {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu_vec_buffer"),
                size: INIT_CAPACITY,
                usage,
                mapped_at_creation: false,
            }),
            length: 0,
        }
    }

    /// Will panic if length is 0.
    pub fn get_writer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        length: wgpu::BufferAddress,
    ) -> wgpu::QueueWriteBufferView {
        self.length = length;
        let capacity = self.buffer.size();

        if length > capacity {
            let size = length.max(capacity * 2);
            self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu_vec_buffer"),
                size,
                usage: self.buffer.usage(),
                mapped_at_creation: false,
            });
        }

        queue
            .write_buffer_with(
                &self.buffer,
                0,
                NonZeroU64::new(length).expect("length must be not 0"),
            )
            .unwrap()
    }

    pub fn get_slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(.. self.length)
    }
}
