use crate::{
    component::actor_model::ActorModelComponent,
    entity::actor_model::{
        ActorAnimation,
        ActorBodyPart,
    },
    system::render::primitives::{
        Polygon,
        Vertex,
    },
};
use anyhow::Error;
use arrayvec::ArrayVec;
use nohash_hasher::IntMap;
use serde::Deserialize;
use std::collections::BTreeMap;
use voxbrix_common::{
    component::actor::position::Position,
    math::{
        Mat4F32,
        QuatF32,
        Round,
        Vec3F32,
    },
    ArrayExt,
    LabelMap,
};

pub const BASE_BODY_PART: ActorBodyPart = ActorBodyPart(0);
const VERTEX_TEXTURE_POSITION_OFFSET: f32 = 0.01;

pub type BuilderActorModelComponent = ActorModelComponent<ActorModelBuilder>;

pub struct ActorModelBuilder {
    center: Vec3F32,
    default_scale: f32,
    texture: u32,
    body_parts: IntMap<ActorBodyPart, ActorBodyPartBuilder>,
    animations: IntMap<ActorAnimation, ActorAnimationBuilder>,
}

impl ActorModelBuilder {
    pub fn list_body_parts(&self) -> impl ExactSizeIterator<Item = &ActorBodyPart> {
        self.body_parts.keys()
    }

    pub fn build_body_part(
        &self,
        body_part: &ActorBodyPart,
        position: &Position,
        transform: &Mat4F32,
        polygons: &mut Vec<Polygon>,
    ) {
        let body_part_builder = match self.body_parts.get(body_part) {
            Some(s) => s,
            None => {
                return;
            },
        };

        polygons.extend(body_part_builder.polygons.iter().map(|vertices| {
            Polygon {
                chunk: position.chunk.position.into(),
                texture_index: self.texture,
                vertices: vertices
                    .iter()
                    .map(|vertex| {
                        Vertex {
                            position: (position.offset
                                + transform.transform_point3(vertex.position - self.center)
                                    * self.default_scale)
                                .into(),
                            texture_position: vertex.texture_position,
                            light_level: [15, 0, 0, 0],
                        }
                    })
                    .collect::<ArrayVec<_, 4>>()
                    .into_inner()
                    .unwrap(),
            }
        }));
    }

    /// `time` must be in `(0 ..= 1)`
    pub fn animate_body_part(
        &self,
        body_part: &ActorBodyPart,
        animation: &ActorAnimation,
        time: f32,
    ) -> Option<Transformation> {
        let animation_builder = self.animations.get(animation)?;

        let body_part = *body_part;

        let time = time * animation_builder.duration;
        let time_key: Time = time.round_down() as Time;

        let prev_frame = animation_builder
            .transformations
            .range((body_part, Time::MIN) ..= (body_part, time_key))
            .rev()
            .next();

        let next_frame = animation_builder
            .transformations
            .range((body_part, time_key + 1) .. (body_part, Time::MAX))
            .next();

        let (((_, prev_time_key), prev_frame), ((_, next_time_key), next_frame)) =
            match (prev_frame, next_frame) {
                (Some(p), Some(n)) => (p, n),
                (Some((_, only)), None) | (None, Some((_, only))) => return Some(*only),
                (None, None) => return None,
            };

        let interp_amount =
            (time - *prev_time_key as f32) / (next_time_key - *prev_time_key) as f32;

        Some(Transformation {
            scale: prev_frame.scale.lerp(next_frame.scale, interp_amount),
            rotate: prev_frame.rotate.slerp(next_frame.rotate, interp_amount),
            translate: prev_frame
                .translate
                .lerp(next_frame.translate, interp_amount),
        })
    }

    pub fn has_animation(&self, animation: &ActorAnimation) -> bool {
        self.animations.get(&animation).is_some()
    }

    pub fn get_body_part_parent(&self, body_part: &ActorBodyPart) -> Option<ActorBodyPart> {
        self.body_parts.get(body_part).map(|bp| bp.parent)
    }
}

pub struct ActorModelBuilderContext<'a> {
    pub actor_texture_label_map: &'a LabelMap<u32>,
    pub actor_body_part_label_map: &'a LabelMap<ActorBodyPart>,
    pub actor_animation_label_map: &'a LabelMap<ActorAnimation>,
}

#[derive(Deserialize, Debug)]
pub struct ActorModelBuilderDescriptor {
    grid_size: [usize; 3],
    grid_in_block: usize,
    texture_label: String,
    texture_grid_size: [usize; 2],
    body_parts: BTreeMap<String, ActorBodyPartDescriptor>,
    animations: BTreeMap<String, ActorAnimationDescriptor>,
}

impl ActorModelBuilderDescriptor {
    pub fn describe(&self, ctx: &ActorModelBuilderContext) -> Result<ActorModelBuilder, Error> {
        let center: Vec3F32 = self.grid_size.map(|s| s as f32 / 2.0).into();
        let center_translate = Mat4F32::from_translation(center);
        let center_translate_inv = Mat4F32::from_translation(-center);

        let default_scale = 1.0 / (self.grid_in_block as f32);

        let texture = ctx
            .actor_texture_label_map
            .get(&self.texture_label)
            .ok_or_else(|| {
                Error::msg(format!("texture \"{}\" is undefined", self.texture_label))
            })?;

        let body_parts = self
            .body_parts
            .iter()
            .map(|(label, desc)| {
                let body_part = ctx
                    .actor_body_part_label_map
                    .get(&label)
                    .ok_or_else(|| Error::msg(format!("body part \"{}\" is undefined", label)))?;

                let parent = ctx
                    .actor_body_part_label_map
                    .get(&desc.parent_label)
                    .ok_or_else(|| {
                        Error::msg(format!(
                            "parent \"{}\" of body part \"{}\" is undefined",
                            desc.parent_label, label,
                        ))
                    })?;

                if parent != BASE_BODY_PART && self.body_parts.get(&desc.parent_label).is_none() {
                    return Err(Error::msg(format!(
                        "parent \"{}\" of body part \"{}\" is not part of the model",
                        desc.parent_label, label,
                    )));
                }

                let polygons = desc
                    .sides
                    .iter()
                    .map(|side| {
                        // Here is the fix for the glitchy pixels on the edge of sides
                        // (just cause those grinded my gears)
                        // Sometimes pixels on the edge of a side appear to be sampled from the outside
                        // of the designated texture area, it happens because of f32 texture position inaccuracy
                        // To compensate, find the center of the side in texture surface and move every
                        // vertex toward that center by VERTEX_TEXTURE_POSITION_OFFSET fracture
                        // of the grid size
                        // Grid size involved in correction to have approximately the same offset even for
                        // non-square textures
                        let texture_coords_sum =
                            side.iter().fold([0.0, 0.0], |[sum_x, sum_y], vertex| {
                                let x_float = (vertex.texture_position[0] as f32)
                                    / (self.texture_grid_size[0] as f32);
                                let y_float = (vertex.texture_position[1] as f32)
                                    / (self.texture_grid_size[1] as f32);
                                [x_float + sum_x, y_float + sum_y]
                            });

                        let side_texture_center = texture_coords_sum.map(|sum| sum / 4.0);

                        side.map_ref(|vertex| {
                            let ActorBodyPartDescriptorVertex {
                                position,
                                texture_position,
                            } = vertex;

                            let get_texture_position = |axis| {
                                let grid_size = self.texture_grid_size[axis] as f32;

                                let texture_position = (texture_position[axis] as f32) / grid_size;

                                let correction_amplitude =
                                    VERTEX_TEXTURE_POSITION_OFFSET / grid_size;

                                let correction_sign = side_texture_center[axis] - texture_position;

                                texture_position + correction_amplitude.copysign(correction_sign)
                            };

                            ActorBodyPartVertex {
                                position: position.map(|pos| pos as f32).into(),
                                texture_position: [
                                    get_texture_position(0),
                                    get_texture_position(1),
                                ],
                            }
                        })
                    })
                    .collect();

                let builder = ActorBodyPartBuilder { parent, polygons };

                Ok((body_part, builder))
            })
            .collect::<Result<_, Error>>()?;

        let animations = self
            .animations
            .iter()
            .map(|(label, desc)| {
                let animation = ctx
                    .actor_animation_label_map
                    .get(&label)
                    .ok_or_else(|| Error::msg(format!("animation \"{}\" is undefined", label)))?;

                let mut transformations = BTreeMap::new();

                for transform_desc in desc.transformations.iter() {
                    let TransformationDescriptor {
                        time,
                        body_part,
                        operations,
                    } = transform_desc;

                    let body_part =
                        ctx.actor_body_part_label_map
                            .get(&body_part)
                            .ok_or_else(|| {
                                Error::msg(format!(
                                    "body part \"{}\" in animation \"{}\" is undefined",
                                    body_part, label
                                ))
                            })?;

                    let transform_mat = match transformations.get_mut(&(body_part, time)) {
                        Some(t) => t,
                        None => {
                            transformations.insert((body_part, time), Mat4F32::IDENTITY);
                            transformations.get_mut(&(body_part, time)).unwrap()
                        },
                    };

                    for operation in operations {
                        let operation = match operation {
                            Operation::Scale(oper) => Mat4F32::from_scale(*oper),
                            Operation::Rotate {
                                axis,
                                angle_degrees,
                            } => {
                                let oper =
                                    QuatF32::from_axis_angle(*axis, angle_degrees.to_radians());
                                Mat4F32::from_quat(oper)
                            },
                            Operation::Translate(oper) => Mat4F32::from_translation(*oper),
                        };

                        *transform_mat = operation * *transform_mat;
                    }
                }

                let builder = ActorAnimationBuilder {
                    duration: desc.duration as f32,
                    transformations: transformations
                        .iter()
                        .map(|(key, transform)| {
                            let transform = center_translate_inv * *transform * center_translate;

                            (key, transform)
                        })
                        .map(|((model, anim), transform)| {
                            ((*model, **anim), Transformation::from_matrix(&transform))
                        })
                        .collect(),
                };

                Ok((animation, builder))
            })
            .collect::<Result<_, Error>>()?;

        Ok(ActorModelBuilder {
            center,
            default_scale,
            texture,
            body_parts,
            animations,
        })
    }
}

#[derive(Deserialize, Debug)]
struct ActorBodyPartVertex {
    position: Vec3F32,
    texture_position: [f32; 2],
}

struct ActorBodyPartBuilder {
    parent: ActorBodyPart,
    polygons: Vec<[ActorBodyPartVertex; 4]>,
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

type Time = u32;

#[derive(Clone, Copy, Debug)]
pub struct Transformation {
    pub scale: Vec3F32,
    pub rotate: QuatF32,
    pub translate: Vec3F32,
}

impl Transformation {
    pub fn from_matrix(mat: &Mat4F32) -> Self {
        let (scale, rotate, translate) = mat.to_scale_rotation_translation();
        Transformation {
            scale,
            rotate,
            translate,
        }
    }

    pub fn to_matrix(&self) -> Mat4F32 {
        Mat4F32::from_scale_rotation_translation(self.scale, self.rotate, self.translate)
    }
}

struct ActorAnimationBuilder {
    duration: f32,
    transformations: BTreeMap<(ActorBodyPart, Time), Transformation>,
}

#[derive(Deserialize, Debug)]
pub struct ActorAnimationDescriptor {
    label: String,
    duration: u16,
    transformations: Vec<TransformationDescriptor>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", content = "value")]
pub enum Operation {
    Scale(Vec3F32),
    Rotate { axis: Vec3F32, angle_degrees: f32 },
    Translate(Vec3F32),
}

#[derive(Deserialize, Debug)]
pub struct TransformationDescriptor {
    time: Time,
    body_part: String,
    operations: Vec<Operation>,
}
