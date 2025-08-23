use anyhow::Error;
use serde::Deserialize;
use voxbrix_common::{
    entity::{
        effect::Effect,
        script::Script,
    },
    system::component_map::ComponentMap,
    AsFromUsize,
    LabelLibrary,
};

const COMPONENT_NAME: &str = "snapshot_handler";

pub struct SnapshotHandlerEffectComponent(Vec<HandlerSet>);

impl SnapshotHandlerEffectComponent {
    pub fn new(
        component_map: &ComponentMap<Effect>,
        label_library: &LabelLibrary,
    ) -> Result<Self, Error> {
        let mut vec = Vec::new();

        vec.resize_with(
            label_library
                .get_label_map_for::<Effect>()
                .expect("Effect label map is undefined")
                .len(),
            HandlerSet::noop,
        );

        for res in component_map.get_component::<HandlerSetDescriptor>(COMPONENT_NAME) {
            let (e, d) = res?;

            vec[e.as_usize()] = d.describe(label_library)?;
        }

        Ok(Self(vec))
    }

    pub fn get(&self, effect: &Effect) -> &HandlerSet {
        self.0.get(effect.as_usize()).unwrap()
    }
}

pub enum Condition {
    Always,
    EveryNSnapshot,
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

pub enum Alteration {
    RemoveThisEffect,
    Scripted { script: Script },
}

pub struct Handler {
    pub condition: Condition,
    pub alterations: Vec<Alteration>,
}

pub struct HandlerSet(Vec<Handler>);

impl HandlerSet {
    pub const fn noop() -> Self {
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
    /// This expects that effect has start ServerSnapshot and duration (u32) written to
    /// state sequentially in that order.
    EveryNSnapshot,
    And {
        set: Vec<ConditionDescriptor>,
    },
    Or {
        set: Vec<ConditionDescriptor>,
    },
}

impl ConditionDescriptor {
    fn describe(&self, label_lib: &LabelLibrary) -> Result<Condition, Error> {
        Ok(match self {
            Self::Always => Condition::Always,
            Self::EveryNSnapshot => Condition::EveryNSnapshot,
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
    pub fn describe(&self, label_lib: &LabelLibrary) -> Result<HandlerSet, Error> {
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
    RemoveThisEffect,
    Scripted { script: String },
}

impl AlterationDescriptor {
    fn describe(&self, label_lib: &LabelLibrary) -> Result<Alteration, Error> {
        Ok(match self {
            Self::RemoveThisEffect => Alteration::RemoveThisEffect,
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
