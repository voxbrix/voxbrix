use crate::assets::ACTION_HANDLER_DIR;
use anyhow::{
    Context,
    Error,
};
use initial::HandlerSet;
pub use initial::HandlerSetDescriptor;
use std::path::PathBuf;
use voxbrix_common::{
    entity::action::Action,
    parse_file_async,
    AsFromUsize,
    LabelLibrary,
    CONFIG_EXTENSION,
};

pub mod initial;
pub mod projectile;

pub struct HandlerActionComponent(Vec<HandlerSet>);

impl HandlerActionComponent {
    pub async fn load<'a>(label_library: &LabelLibrary) -> Result<Self, Error> {
        let label_map = label_library
            .get_label_map_for::<Action>()
            .expect("action label map is undefined");

        let mut vec = Vec::with_capacity(label_map.len());

        for (i, (a, label)) in label_map.iter().enumerate() {
            assert_eq!(
                a.as_usize(),
                i,
                "label map iter must return actions with sequential indices"
            );

            let mut path: PathBuf = ACTION_HANDLER_DIR.into();
            path.push([label, CONFIG_EXTENSION].join("."));

            let desc = parse_file_async::<HandlerSetDescriptor>(path)
                .await
                .with_context(|| format!("no handler for action \"{}\"", label))?;

            let hs = desc
                .describe(label_library)
                .with_context(|| format!("parsing handler for action \"{}\"", label))?;

            vec.push(hs);
        }

        Ok(Self(vec))
    }

    pub fn get(&self, action: &Action) -> &HandlerSet {
        self.0
            .get(action.as_usize())
            .expect("handler must be defined for all actions")
    }
}
