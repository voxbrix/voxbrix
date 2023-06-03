use crate::{
    component::actor_model::{
        animation::{
            ActorAnimationDescriptor,
            AnimationActorModelComponent,
        },
        body_part::{
            ActorBodyPartDescriptor,
            BodyPartActorModelComponent,
        },
    },
    entity::actor_model::{
        ActorAnimation,
        ActorBodyPart,
        ActorModel,
    },
};
use anyhow::{
    Context,
    Error,
};
use serde::Deserialize;
use std::{
    collections::{
        BTreeMap,
        BTreeSet,
    },
    path::Path,
};
use tokio::task;
use voxbrix_common::{
    read_ron_file,
    LabelMap,
};

const MODELS_PATH: &str = "assets/client/models/actors";
const BODY_PART_LIST_PATH: &str = "assets/client/models/actor_body_parts.ron";
const ANIMATION_LIST_PATH: &str = "assets/client/models/actor_animations.ron";
const MODEL_LIST_PATH: &str = "assets/client/models/actors.ron";
const BASE_BODY_PART: ActorBodyPart = ActorBodyPart(0);

#[derive(Deserialize, Debug)]
struct List {
    list: Vec<String>,
}

pub struct BodyPartContext<'a> {
    pub body_part_label_map: &'a LabelMap<ActorBodyPart>,
    pub model_body_part_labels: BTreeSet<String>,
    pub grid_size: [usize; 3],
    pub grid_in_block: usize,
    pub texture: u32,
    pub texture_grid_size: [usize; 2],
}

#[derive(Deserialize, Debug)]
struct ActorModelDescriptor {
    label: String,
    grid_size: [usize; 3],
    grid_in_block: usize,
    texture_label: String,
    texture_grid_size: [usize; 2],
    body_parts: BTreeMap<String, ActorBodyPartDescriptor>,
    animations: BTreeMap<String, ActorAnimationDescriptor>,
}

pub struct ActorModelLoadingSystem {
    pub model_label_map: LabelMap<ActorModel>,
    pub body_part_label_map: LabelMap<ActorBodyPart>,
    pub animation_label_map: LabelMap<ActorAnimation>,
}

impl ActorModelLoadingSystem {
    pub async fn load_data(
        texture_label_map: &LabelMap<u32>,
        body_part_amc: &mut BodyPartActorModelComponent,
        animation_amc: &mut AnimationActorModelComponent,
    ) -> Result<Self, Error> {
        let (model_label_map, model_desc_map, body_part_label_map, animation_label_map) =
            task::spawn_blocking(|| {
                let body_part_label_map = read_ron_file::<List>(BODY_PART_LIST_PATH)?
                    .list
                    .into_iter()
                    .enumerate()
                    .map(|(i, n)| (n, ActorBodyPart(i)))
                    .collect();

                let animation_label_map: LabelMap<ActorAnimation> =
                    read_ron_file::<List>(ANIMATION_LIST_PATH)?
                        .list
                        .into_iter()
                        .enumerate()
                        .map(|(i, n)| (n, ActorAnimation(i)))
                        .collect();

                let model_list: List = read_ron_file(MODEL_LIST_PATH)?;

                let (model_label_map, model_desc_map): (BTreeMap<_, _>, BTreeMap<_, _>) =
                    model_list
                        .list
                        .into_iter()
                        .enumerate()
                        .map(|(actor_model, actor_model_label)| {
                            let actor_model = ActorModel(actor_model);
                            let file_name = format!("{}.ron", actor_model_label);
                            let model_desc: ActorModelDescriptor =
                                read_ron_file(Path::new(MODELS_PATH).join(file_name))?;
                            Ok(((actor_model_label, actor_model), (actor_model, model_desc)))
                        })
                        .collect::<Result<Vec<_>, Error>>()?
                        .into_iter()
                        .unzip();

                Ok::<_, Error>((
                    model_label_map.into(),
                    model_desc_map,
                    body_part_label_map,
                    animation_label_map,
                ))
            })
            .await
            .unwrap()?;

        for (actor_model, model_desc) in model_desc_map {
            let texture = texture_label_map
                .get(&model_desc.texture_label)
                .ok_or_else(|| {
                    Error::msg(format!(
                        "actor model \"{}\" uses undefined texture \"{}\"",
                        model_desc.label, model_desc.texture_label
                    ))
                })?;

            let context = BodyPartContext {
                body_part_label_map: &body_part_label_map,
                model_body_part_labels: model_desc.body_parts.keys().cloned().collect(),
                grid_size: model_desc.grid_size,
                grid_in_block: model_desc.grid_in_block,
                texture_grid_size: model_desc.texture_grid_size,
                texture,
            };

            for (body_part_label, body_part_desc) in model_desc.body_parts {
                let body_part = body_part_label_map.get(&body_part_label).ok_or_else(|| {
                    Error::msg(format!(
                        "in actor model \"{}\" body part \"{}\" is not defined in {:?}",
                        model_desc.label, &body_part_label, BODY_PART_LIST_PATH
                    ))
                })?;

                let body_part_builder = body_part_desc
                    .describe(&context)
                    .with_context(|| format!("in actor model \"{}\"", model_desc.label))?;

                body_part_amc.insert(actor_model, body_part, body_part_builder);
            }

            for (animation_label, animation_desc) in model_desc.animations {
                let animaton = animation_label_map.get(&animation_label).ok_or_else(|| {
                    Error::msg(format!(
                        "in actor model \"{}\" animation \"{}\" is not defined in {:?}",
                        model_desc.label, &animation_label, ANIMATION_LIST_PATH
                    ))
                })?;

                let animation_builder = animation_desc
                    .describe(&body_part_label_map)
                    .with_context(|| format!("in actor model \"{}\"", model_desc.label))?;

                animation_amc.insert(actor_model, animaton, animation_builder);
            }
        }

        Ok(Self {
            model_label_map,
            body_part_label_map,
            animation_label_map,
        })
    }
}
