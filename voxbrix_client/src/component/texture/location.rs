use crate::entity::texture::Texture;
use voxbrix_common::AsFromUsize;

#[derive(Clone)]
pub struct Location {
    pub data_index: u32,
    pub position: [u32; 2],
    pub size: [u32; 2],
    pub edge_correction: [f32; 2],
}

pub struct LocationTextureComponent {
    atlas_size: [u32; 2],
    locations: Vec<Location>,
}

impl LocationTextureComponent {
    pub fn new() -> Self {
        Self {
            atlas_size: [0; 2],
            locations: Vec::new(),
        }
    }

    pub fn load(&mut self, atlas_size: [u32; 2], locations: Vec<Location>) {
        *self = Self {
            atlas_size,
            locations,
        }
    }

    pub fn get_coords(&self, texture: Texture, coords: [f32; 2]) -> [f32; 2] {
        let e = self
            .locations
            .get(texture.as_usize())
            .expect("texture not found");

        [0, 1].map(|i| {
            ((e.position[i] as f64 + e.size[i] as f64 * coords[i] as f64)
                / self.atlas_size[i] as f64) as f32
        })
    }

    pub fn get_index(&self, texture: Texture) -> u32 {
        self.locations
            .get(texture.as_usize())
            .expect("texture not found")
            .data_index
    }

    pub fn get_edge_correction(&self, texture: Texture) -> [f32; 2] {
        self.locations
            .get(texture.as_usize())
            .expect("texture not found")
            .edge_correction
    }
}
