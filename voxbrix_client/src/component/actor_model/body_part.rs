use crate::{
    component::actor_model::ActorModelComponent,
    entity::actor_model::ActorBodyPart,
    system::{
        actor_model_loading::BodyPartContext,
        render::vertex::Vertex,
    },
};
use anyhow::Error;
use serde::Deserialize;
use voxbrix_common::component::actor::position::Position;

const BASE_BODY_PART: ActorBodyPart = ActorBodyPart(0);
const VERTEX_TEXTURE_POSITION_OFFSET: f32 = 0.01;

pub type BodyPartActorModelComponent = ActorModelComponent<ActorBodyPart, ActorBodyPartBuilder>;

#[derive(Deserialize, Debug)]
struct ActorBodyPartVertex {
    position: [f32; 3],
    texture_position: [f32; 2],
}

pub struct ActorBodyPartBuilder {
    parent: ActorBodyPart,
    texture: u32,
    vertices: Vec<ActorBodyPartVertex>,
    indices: Vec<u32>,
}

impl ActorBodyPartBuilder {
    pub fn build(&self, position: &Position, vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>) {
        let index_offset = vertices.len() as u32;

        vertices.extend(self.vertices.iter().map(|vertex| {
            Vertex {
                chunk: position.chunk.position.into(),
                position: (position.offset + vertex.position.into()).into(),
                texture_index: self.texture,
                texture_position: vertex.texture_position,
                light_level: [15, 0, 0, 0],
            }
        }));

        indices.extend(self.indices.iter().map(|i| index_offset + *i));
    }
}

#[derive(Deserialize, Debug)]
struct ActorBodyPartDescriptorVertex {
    position: [usize; 3],
    texture_position: [usize; 2],
}

#[derive(Deserialize, Debug)]
pub struct ActorBodyPartDescriptor {
    label: String,
    parent_label: String,
    sides: Vec<[ActorBodyPartDescriptorVertex; 4]>,
}

impl ActorBodyPartDescriptor {
    pub fn describe(self, ctx: &BodyPartContext) -> Result<ActorBodyPartBuilder, Error> {
        let parent = ctx
            .body_part_label_map
            .get(&self.parent_label)
            .ok_or_else(|| {
                Error::msg(format!(
                    "unable to describe {}: parent with label {} is undefined",
                    self.label, self.parent_label
                ))
            })?;

        if parent != BASE_BODY_PART && !ctx.model_body_part_labels.contains(&self.parent_label) {
            return Err(Error::msg(format!(
                "parent {} of the body part {} is undefined in the model",
                self.parent_label, self.label
            )));
        }

        let mut vertices = Vec::with_capacity(self.sides.len() * 4);
        let mut indices = Vec::with_capacity(self.sides.len() * 6);

        for side in self.sides {
            let index_offset = vertices.len() as u32;

            // Here is the fix for the glitchy pixels on the edge of sides
            // (just cause those grinded my gears)
            // Sometimes pixels on the edge of a side appear to be sampled from the outside
            // of the designated texture area, it happens because of f32 texture position inaccuracy
            // To compensate, find the center of the side in texture surface and move every
            // vertex toward that center by VERTEX_TEXTURE_POSITION_OFFSET fracture
            // of the grid size
            // Grid size involved in correction to have approximately the same offset even for
            // non-square textures
            let texture_coords_sum = side.iter().fold([0.0, 0.0], |[sum_x, sum_y], vertex| {
                let x_float =
                    (vertex.texture_position[0] as f32) / (ctx.texture_grid_size[0] as f32);
                let y_float =
                    (vertex.texture_position[1] as f32) / (ctx.texture_grid_size[1] as f32);
                [x_float + sum_x, y_float + sum_y]
            });

            let side_texture_center = texture_coords_sum.map(|sum| sum / 4.0);

            for ActorBodyPartDescriptorVertex {
                position,
                texture_position,
            } in side.into_iter()
            {
                let grid_resolution = 1.0 / (ctx.grid_in_block as f32);

                let get_texture_position = |axis| {
                    let grid_size = ctx.texture_grid_size[axis] as f32;

                    let texture_position = (texture_position[axis] as f32) / grid_size;

                    let correction_amplitude = VERTEX_TEXTURE_POSITION_OFFSET / grid_size;

                    let correction_sign = side_texture_center[axis] - texture_position;

                    texture_position + correction_amplitude.copysign(correction_sign)
                };

                vertices.push(ActorBodyPartVertex {
                    position: position.map(|pos| pos as f32 * grid_resolution),
                    texture_position: [get_texture_position(0), get_texture_position(1)],
                });
            }

            indices.extend([0, 1, 3, 2, 3, 1].map(|i| index_offset + i));
        }

        Ok(ActorBodyPartBuilder {
            parent,
            texture: ctx.texture,
            vertices,
            indices,
        })
    }
}
