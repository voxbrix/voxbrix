use anyhow::{
    Context,
    Error,
};
use serde::Deserialize;
use voxbrix_common::{
    entity::{
        action::Action,
        actor_class::ActorClass,
        effect::Effect,
        script::Script,
    },
    AsFromUsize,
    LabelLibrary,
};

pub struct HandlerActionComponent(Vec<HandlerSet>);

impl HandlerActionComponent {
    pub fn load_from_descriptor<'a>(
        label_library: &LabelLibrary,
        get_descriptor: &'a dyn Fn(&str) -> Option<&'a HandlerSetDescriptor>,
    ) -> Result<Self, Error> {
        let label_map = label_library
            .get_label_map_for::<Action>()
            .expect("action label map is undefined");

        let iter = label_map.iter().enumerate().map(|(i, (a, label))| {
            assert_eq!(
                a.as_usize(),
                i,
                "label map iter must return actions with sequential indices"
            );

            get_descriptor(label)
                .map(|desc| desc.describe(label_library))
                .unwrap_or(Ok(HandlerSet::noop()))
                .with_context(|| format!("parsing handler for action \"{}\"", label))
        });

        let mut vec = Vec::with_capacity(label_map.len());

        for result in iter {
            vec.push(result?);
        }

        Ok(Self(vec))
    }

    pub fn get(&self, action: &Action) -> &HandlerSet {
        self.0
            .get(action.as_usize())
            .expect("handler must be defined for all actions")
    }
}

pub enum Condition {
    Always,
    SourceActorHasNoEffect(Effect),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

#[derive(Clone, Copy, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Source {
    Actor,
    World,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Target {
    Actor,
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
    CreateProjectile {
        actor_class: ActorClass,
    },
    Scripted {
        script: Script,
    },
}

pub struct Handler {
    pub condition: Condition,
    pub alterations: Vec<Alteration>,
}

pub struct HandlerSet(Vec<Handler>);

impl HandlerSet {
    const fn noop() -> Self {
        Self(Vec::new())
    }

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
    condition: ConditionDescriptor,
    alterations: Vec<AlterationDescriptor>,
}

impl HandlerDescriptor {
    fn describe(&self, label_lib: &LabelLibrary) -> Result<Handler, Error> {
        Ok(Handler {
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
    fn describe(&self, label_lib: &LabelLibrary) -> Result<HandlerSet, Error> {
        let set = self
            .0
            .iter()
            .map(|d| d.describe(label_lib))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(HandlerSet(set))
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
    CreateProjectile {
        actor_class: String,
    },
    Scripted {
        script: String,
    },
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
            Self::CreateProjectile { actor_class } => {
                Alteration::CreateProjectile {
                    actor_class: label_lib.get(&actor_class).ok_or_else(|| {
                        anyhow::anyhow!("actor class \"{}\" is undefined", actor_class)
                    })?,
                }
            },
            Self::Scripted { script } => {
                Alteration::Scripted {
                    script: label_lib
                        .get(&script)
                        .ok_or_else(|| anyhow::anyhow!("script \"{}\" is undefined", script))?,
                }
            },
        })
    }
}
