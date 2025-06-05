use crate::{
    component::{
        actor_model::ActorModelComponent,
        texture::location::LocationTextureComponent,
    },
    entity::{
        actor_model::{
            ActorAnimation,
            ActorBone,
        },
        texture::Texture,
    },
    resource::render_pool::primitives::Vertex,
};
use anyhow::Error;
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

pub const BASE_BONE: ActorBone = ActorBone(0);

pub type BuilderActorModelComponent = ActorModelComponent<ActorModelBuilder>;

pub struct ActorModelBuilder {
    default_scale: f32,
    texture: u32,
    skeleton: IntMap<ActorBone, BoneParameters>,
    model_parts: IntMap<ActorBone, ActorModelPartBuilder>,
    animations: IntMap<ActorAnimation, ActorAnimationBuilder>,
}

impl ActorModelBuilder {
    pub fn list_bones(&self) -> impl ExactSizeIterator<Item = &ActorBone> {
        self.skeleton.keys()
    }

    pub fn build_bone(
        &self,
        bone: &ActorBone,
        position: &Position,
        transform: &Mat4F32,
        vertices: &mut Vec<Vertex>,
    ) {
        if self.skeleton.get(bone).is_none() {
            return;
        };

        // TODO external model part replacements & additions
        let model_part_builder = match self.model_parts.get(bone) {
            Some(s) => s,
            None => {
                return;
            },
        };

        let transform = *transform * model_part_builder.transformation;

        vertices.extend(model_part_builder.quads.iter().flat_map(|vertices| {
            vertices.map_ref(|vertex| {
                Vertex {
                    chunk: position.chunk.position,
                    texture_index: self.texture,
                    offset: (position.offset
                        + transform.transform_point3(vertex.position) * self.default_scale)
                        .into(),
                    texture_position: vertex.texture_position,
                    light_parameters: 0,
                }
            })
        }));
    }

    /// `time` must be in `(0 ..= 1)`
    pub fn animate_bone(
        &self,
        bone: &ActorBone,
        animation: &ActorAnimation,
        time: f32,
    ) -> Option<Transformation> {
        let animation_builder = self.animations.get(animation)?;

        let bone = *bone;

        let time = time * animation_builder.duration;
        let time_key: Time = time.round_down() as Time;

        let prev_frame = animation_builder
            .transformations
            .range((bone, Time::MIN) ..= (bone, time_key))
            .rev()
            .next();

        let next_frame = animation_builder
            .transformations
            .range((bone, time_key + 1) .. (bone, Time::MAX))
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

    pub fn get_bone_parameters(&self, bone: &ActorBone) -> Option<&BoneParameters> {
        self.skeleton.get(bone)
    }
}

pub struct ActorModelBuilderContext<'a> {
    pub texture_label_map: LabelMap<Texture>,
    pub location_tc: &'a LocationTextureComponent,
    pub actor_bone_label_map: &'a LabelMap<ActorBone>,
    pub actor_animation_label_map: &'a LabelMap<ActorAnimation>,
}

#[derive(Deserialize, Debug)]
pub struct ActorModelBuilderDescriptor {
    grid_in_block: usize,
    texture_label: String,
    texture_grid_size: [usize; 2],
    skeleton: BTreeMap<String, ActorBoneDescriptor>,
    model_parts: BTreeMap<String, ActorModelPartDescriptor>,
    animations: BTreeMap<String, ActorAnimationDescriptor>,
}

impl ActorModelBuilderDescriptor {
    pub fn describe(&self, ctx: &ActorModelBuilderContext) -> Result<ActorModelBuilder, Error> {
        let default_scale = 1.0 / (self.grid_in_block as f32);

        let texture = ctx
            .texture_label_map
            .get(&self.texture_label)
            .ok_or_else(|| {
                Error::msg(format!("texture \"{}\" is undefined", self.texture_label))
            })?;

        let skeleton = self
            .skeleton
            .iter()
            .map(|(label, desc)| {
                let bone = ctx
                    .actor_bone_label_map
                    .get(&label)
                    .ok_or_else(|| Error::msg(format!("bone \"{}\" is undefined", label)))?;

                let parent = ctx.actor_bone_label_map.get(&desc.parent).ok_or_else(|| {
                    Error::msg(format!(
                        "parent \"{}\" of bone \"{}\" is undefined",
                        desc.parent, label,
                    ))
                })?;

                if parent != BASE_BONE && self.skeleton.get(&desc.parent).is_none() {
                    return Err(Error::msg(format!(
                        "parent \"{}\" of bone \"{}\" is not part of the model",
                        desc.parent, label,
                    )));
                }

                let mut transformation = Mat4F32::IDENTITY;

                for operation in desc.transformations.iter() {
                    transformation = operation.to_matrix() * transformation;
                }

                let builder = BoneParameters {
                    parent,
                    transformation,
                };

                Ok((bone, builder))
            })
            .collect::<Result<_, Error>>()?;

        let model_parts = self
            .model_parts
            .iter()
            .map(|(label, desc)| {
                let bone = ctx.actor_bone_label_map.get(&label).ok_or_else(|| {
                    Error::msg(format!("bone \"{}\" for model part is undefined", label))
                })?;

                if bone != BASE_BONE && self.skeleton.get(label.as_str()).is_none() {
                    return Err(Error::msg(format!(
                        "bone \"{}\" is not part of the model, cannot attach model part to it",
                        label,
                    )));
                }

                let quads = desc
                    .quads
                    .iter()
                    .map(|quad| {
                        // Here is the fix for the glitchy pixels on the edge of quads
                        // (just cause those grinded my gears)
                        // Sometimes pixels on the edge of a quad appear to be sampled from the outside
                        // of the designated texture area, it happens because of f32 texture position inaccuracy
                        // To compensate, find the center of the quad in texture surface and move every
                        // vertex toward that center by VERTEX_TEXTURE_POSITION_OFFSET fracture
                        // of the grid size
                        // Grid size involved in correction to have approximately the same offset even for
                        // non-square textures

                        let texture_coords_sum = quad.iter().fold([0.0, 0.0], |sum, vertex| {
                            let coords = [0, 1].map(|i| {
                                (vertex.texture_position[i] as f32)
                                    / (self.texture_grid_size[i] as f32)
                            });

                            let coords = ctx.location_tc.get_coords(texture, coords);

                            [0, 1].map(|i| coords[i] + sum[i])
                        });

                        let quad_texture_center = texture_coords_sum.map(|sum| sum / 4.0);

                        quad.map_ref(|vertex| {
                            let ActorModelPartDescriptorVertex {
                                position,
                                texture_position,
                            } = vertex;

                            let texture_position = [0, 1].map(|i| {
                                (texture_position[i] as f32) / self.texture_grid_size[i] as f32
                            });

                            let texture_position =
                                ctx.location_tc.get_coords(texture, texture_position);

                            let correction_amplitude = ctx.location_tc.get_edge_correction(texture);

                            let texture_position = [0, 1].map(|i| {
                                let correction_sign = quad_texture_center[i] - texture_position[i];

                                texture_position[i]
                                    + correction_amplitude[i].copysign(correction_sign)
                            });

                            ActorModelPartVertex {
                                position: position.map(|pos| pos as f32).into(),
                                texture_position,
                            }
                        })
                    })
                    .collect();

                let mut transformation = Mat4F32::IDENTITY;

                for operation in desc.transformations.iter() {
                    transformation = operation.to_matrix() * transformation;
                }

                let builder = ActorModelPartBuilder {
                    quads,
                    transformation,
                };

                Ok((bone, builder))
            })
            .collect::<Result<_, Error>>()?;

        let animations =
            self.animations
                .iter()
                .map(|(label, desc)| {
                    let animation = ctx.actor_animation_label_map.get(&label).ok_or_else(|| {
                        Error::msg(format!("animation \"{}\" is undefined", label))
                    })?;

                    let mut transformations = BTreeMap::new();

                    for transform_desc in desc.transformations.iter() {
                        let TransformationDescriptor {
                            time,
                            bone,
                            operations,
                        } = transform_desc;

                        let bone = ctx.actor_bone_label_map.get(&bone).ok_or_else(|| {
                            Error::msg(format!(
                                "bone \"{}\" in animation \"{}\" is undefined",
                                bone, label
                            ))
                        })?;

                        let transform_mat = match transformations.get_mut(&(bone, time)) {
                            Some(t) => t,
                            None => {
                                transformations.insert((bone, time), Mat4F32::IDENTITY);
                                transformations.get_mut(&(bone, time)).unwrap()
                            },
                        };

                        for operation in operations {
                            *transform_mat = operation.to_matrix() * *transform_mat;
                        }
                    }

                    let builder = ActorAnimationBuilder {
                        duration: desc.duration as f32,
                        transformations: transformations
                            .iter()
                            .map(|((model, anim), transform)| {
                                ((*model, **anim), Transformation::from_matrix(&transform))
                            })
                            .collect(),
                    };

                    Ok((animation, builder))
                })
                .collect::<Result<_, Error>>()?;

        Ok(ActorModelBuilder {
            default_scale,
            texture: ctx.location_tc.get_index(texture),
            skeleton,
            model_parts,
            animations,
        })
    }
}

#[derive(Deserialize, Debug)]
struct ActorModelPartVertex {
    position: Vec3F32,
    texture_position: [f32; 2],
}

pub struct BoneParameters {
    pub parent: ActorBone,
    pub transformation: Mat4F32,
}

struct ActorModelPartBuilder {
    quads: Vec<[ActorModelPartVertex; 4]>,
    transformation: Mat4F32,
}

#[derive(Deserialize, Debug)]
struct ActorModelPartDescriptorVertex {
    position: [isize; 3],
    texture_position: [usize; 2],
}

#[derive(Deserialize, Debug)]
pub struct ActorBoneDescriptor {
    parent: String,
    transformations: Vec<Operation>,
}

#[derive(Deserialize, Debug)]
pub struct ActorModelPartDescriptor {
    quads: Vec<[ActorModelPartDescriptorVertex; 4]>,
    transformations: Vec<Operation>,
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
    transformations: BTreeMap<(ActorBone, Time), Transformation>,
}

#[derive(Deserialize, Debug)]
pub struct ActorAnimationDescriptor {
    duration: u16,
    transformations: Vec<TransformationDescriptor>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "kind")]
pub enum Operation {
    Scale { value: Vec3F32 },
    Rotate { axis: Vec3F32, angle_degrees: f32 },
    Translate { value: Vec3F32 },
}

impl Operation {
    fn to_matrix(&self) -> Mat4F32 {
        match self {
            Operation::Scale { value } => Mat4F32::from_scale(*value),
            Operation::Rotate {
                axis,
                angle_degrees,
            } => {
                let oper = QuatF32::from_axis_angle(*axis, angle_degrees.to_radians());
                Mat4F32::from_quat(oper)
            },
            Operation::Translate { value } => Mat4F32::from_translation(*value),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct TransformationDescriptor {
    time: Time,
    bone: String,
    operations: Vec<Operation>,
}
