use std::num::NonZeroU64;

pub enum BufferMutSlice<'a> {
    Mapped(wgpu::BufferViewMut<'a>),
    Queued(wgpu::QueueWriteBufferView<'a>),
}

impl<'a> AsMut<[u8]> for BufferMutSlice<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Mapped(b) => b.as_mut(),
            Self::Queued(b) => b.as_mut(),
        }
    }
}

pub struct GpuVec {
    buffer: wgpu::Buffer,
    length: wgpu::BufferAddress,
    mapped: bool,
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
                mapped_at_creation: true,
            }),
            length: 0,
            mapped: true,
        }
    }

    /// Will panic if length is 0.
    pub fn get_writer<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &'a wgpu::Queue,
        length: wgpu::BufferAddress,
    ) -> BufferMutSlice<'_> {
        self.length = length;
        let capacity = self.buffer.size();

        if length > capacity {
            let size = length.max(capacity * 2);
            self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpu_vec_buffer"),
                size,
                usage: self.buffer.usage(),
                mapped_at_creation: true,
            });
            self.mapped = true;

            BufferMutSlice::Mapped(self.buffer.slice(.. length).get_mapped_range_mut())
        } else {
            BufferMutSlice::Queued(
                queue
                    .write_buffer_with(
                        &self.buffer,
                        0,
                        NonZeroU64::new(length).expect("length must be not 0"),
                    )
                    .unwrap(),
            )
        }
    }

    pub fn get_slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(.. self.length)
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn finish(&mut self) {
        if self.mapped {
            self.buffer.unmap();
            self.mapped = false;
        }
    }
}
