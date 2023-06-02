use crate::{
    component::actor_model::ActorModelComponent,
    entity::actor_model::ActorAnimation,
    entity::actor_model::ActorBodyPart,
};
use serde::Deserialize;
use voxbrix_common::math::{
    Round,
    Vec3F32,
    QuatF32,
    Mat4F32,
};
use voxbrix_common::LabelMap;
use std::collections::BTreeMap;
use anyhow::Error;

pub type AnimationActorModelComponent = ActorModelComponent<ActorAnimation, ActorAnimationBuilder>;

type Time = u32;

enum PartNode {
    Header(ActorBodyPart),
    Parent(ActorBodyPart),
}

pub struct PartTree {
    
}

#[derive(Clone, Copy, Debug)]
pub struct Transformation {
    pub scale: Vec3F32,
    pub rotate: QuatF32,
    pub translate: Vec3F32,
}

impl Transformation {
    pub fn from_matrix(mat: Mat4F32) -> Self {
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

pub struct ActorAnimationBuilder {
    duration: f32,
    transformations: BTreeMap<(ActorBodyPart, Time), Transformation>,
}

impl ActorAnimationBuilder {
    /// `time` must be in `(0 ..= 1)`
    pub fn of_body_part(&self, body_part: ActorBodyPart, time: f32) -> Option<Transformation> {
        let time: Time = (time * self.duration).round_down() as Time;

        let prev_frame = self.transformations.range((body_part, Time::MIN) .. (body_part, time)).rev().next();
        let next_frame = self.transformations.range((body_part, time) .. (body_part, Time::MAX)).next();

        let (((_, prev_time), prev_frame), ((_, next_time), next_frame)) = match (prev_frame, next_frame) {
            (Some(p), Some(n)) => (p, n),
            (Some((_, only)), None) | (None, Some((_, only))) => return Some(*only), 
            (None, None) => return None,
        };

        let interp_amount = time.abs_diff(*prev_time) as f32 / next_time.abs_diff(*prev_time) as f32;

        Some(Transformation {
            scale: prev_frame.scale.lerp(next_frame.scale, interp_amount),
            rotate: prev_frame.rotate.slerp(next_frame.rotate, interp_amount),
            translate: prev_frame.translate.lerp(next_frame.translate, interp_amount),
        })
    }
}

#[derive(Deserialize, Debug)]
pub struct ActorAnimationDescriptor {
    label: String,
    duration: u16,
    transformations: Vec<TransformationDescriptor>,
}

impl ActorAnimationDescriptor {
    pub fn describe(self, body_part_labels: &LabelMap<ActorBodyPart>) -> Result<ActorAnimationBuilder, Error> {
        let mut transformations = BTreeMap::new();

        for transform_desc in self.transformations {
            let TransformationDescriptor { time, body_part, operations } = transform_desc;

            let body_part = body_part_labels.get(&body_part)
                .ok_or_else(|| {
                    Error::msg(format!(
                        "unable to describe {}: body part with label {} is undefined",
                        self.label, body_part
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
                    Operation::Scale(oper) => Mat4F32::from_scale(oper),
                    Operation::Rotate(oper) => Mat4F32::from_quat(oper),
                    Operation::Translate(oper) => Mat4F32::from_translation(oper),
                };

                *transform_mat = operation * *transform_mat;
            }
        }

        Ok(ActorAnimationBuilder {
            duration: self.duration as f32,
            transformations: transformations.into_iter().map(|(key, transform)| {
                (key, Transformation::from_matrix(transform))
            }).collect(),
        })
    }
}

#[derive(Deserialize, Debug)]
pub enum Operation {
    Scale(Vec3F32),
    Rotate(QuatF32),
    Translate(Vec3F32),
}

#[derive(Deserialize, Debug)]
pub struct TransformationDescriptor {
    time: Time,
    body_part: String,
    operations: Vec<Operation>,
}
