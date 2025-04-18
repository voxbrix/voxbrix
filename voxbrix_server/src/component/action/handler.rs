use anyhow::{
    Context,
    Error,
};
use initial::HandlerSet;
pub use initial::HandlerSetDescriptor;
use voxbrix_common::{
    entity::action::Action,
    AsFromUsize,
    LabelLibrary,
};

pub mod initial;
pub mod projectile;

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
