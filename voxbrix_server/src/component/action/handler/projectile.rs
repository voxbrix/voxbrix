use anyhow::Error;
use serde::Deserialize;
use std::sync::Arc;
use voxbrix_common::{
    entity::{
        effect::Effect,
        script::Script,
    },
    LabelLibrary,
};

pub enum Condition {
    Always,
    SourceActorHasNoEffect(Effect),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

#[derive(Clone, Copy, Deserialize)]
pub enum Source {
    Source,
    World,
    Collider,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Target {
    Source,
    Collider,
    AllInRadius(f32),
}

pub enum Alteration {
    ApplyEffect {
        source: Source,
        target: Target,
        effect: Effect,
    },
    RemoveSourceActorEffect {
        effect: Effect,
    },
    RemoveSelf,
}

#[derive(Clone, Copy, Deserialize)]
pub enum Trigger {
    AnyCollision,
    ActorCollision,
    BlockCollision,
}

pub struct Handler {
    pub trigger: Trigger,
    pub condition: Condition,
    pub alterations: Vec<Alteration>,
}

#[derive(Clone)]
pub struct HandlerSet(Arc<[Handler]>);

impl HandlerSet {
    pub fn iter<'a>(&'a self) -> impl ExactSizeIterator<Item = &'a Handler> + Send + Sync + 'a {
        self.0.iter()
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum ConditionDescriptor {
    Always,
    SourceActorHasNoEffect { effect: String },
    And { set: Vec<ConditionDescriptor> },
    Or { set: Vec<ConditionDescriptor> },
}

impl ConditionDescriptor {
    fn describe(&self, label_lib: &LabelLibrary) -> Result<Condition, Error> {
        Ok(match self {
            Self::Always => Condition::Always,
            Self::SourceActorHasNoEffect { effect } => {
                Condition::SourceActorHasNoEffect(
                    label_lib
                        .get(&effect)
                        .ok_or_else(|| anyhow::anyhow!("effect \"{}\" is undefined", effect))?,
                )
            },
            Self::And { set } => {
                Condition::And(
                    set.into_iter()
                        .map(|c| c.describe(label_lib))
                        .collect::<Result<_, _>>()?,
                )
            },
            Self::Or { set } => {
                Condition::Or(
                    set.into_iter()
                        .map(|c| c.describe(label_lib))
                        .collect::<Result<_, _>>()?,
                )
            },
        })
    }
}

#[derive(Deserialize)]
struct HandlerDescriptor {
    trigger: Trigger,
    condition: ConditionDescriptor,
    alterations: Vec<AlterationDescriptor>,
}

impl HandlerDescriptor {
    fn describe(&self, label_lib: &LabelLibrary) -> Result<Handler, Error> {
        Ok(Handler {
            trigger: self.trigger,
            condition: self.condition.describe(label_lib)?,
            alterations: self
                .alterations
                .iter()
                .map(|a| a.describe(label_lib))
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Deserialize)]
pub struct HandlerSetDescriptor(Vec<HandlerDescriptor>);

impl HandlerSetDescriptor {
    pub fn describe(&self, label_lib: &LabelLibrary) -> Result<HandlerSet, Error> {
        let set = self
            .0
            .iter()
            .map(|d| d.describe(label_lib))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(HandlerSet(set.into()))
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
pub enum AlterationDescriptor {
    ApplyEffect {
        source: Source,
        target: Target,
        effect: String,
    },
    RemoveSourceActorEffect {
        effect: String,
    },
    RemoveSelf,
}

impl AlterationDescriptor {
    fn describe(&self, label_lib: &LabelLibrary) -> Result<Alteration, Error> {
        Ok(match self {
            Self::ApplyEffect {
                target,
                effect,
                source,
            } => {
                Alteration::ApplyEffect {
                    source: *source,
                    target: *target,
                    effect: label_lib
                        .get(&effect)
                        .ok_or_else(|| anyhow::anyhow!("effect \"{}\" is undefined", effect))?,
                }
            },
            Self::RemoveSourceActorEffect { effect } => {
                Alteration::RemoveSourceActorEffect {
                    effect: label_lib
                        .get(&effect)
                        .ok_or_else(|| anyhow::anyhow!("effect \"{}\" is undefined", effect))?,
                }
            },
            Self::RemoveSelf => Alteration::RemoveSelf,
        })
    }
}
