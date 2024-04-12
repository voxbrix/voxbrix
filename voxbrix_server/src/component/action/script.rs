use anyhow::{
    Context,
    Error,
};
use nohash_hasher::IntMap;
use voxbrix_common::{
    entity::{
        action::Action,
        script::Script,
    },
    LabelMap,
};

pub struct ScriptActionComponent(IntMap<Action, Script>);

impl ScriptActionComponent {
    pub fn new<'a>(
        action_script_pairs: impl Iterator<Item = (&'a str, &'a str)>,
        action_label_map: &LabelMap<Action>,
        script_label_map: &LabelMap<Script>,
    ) -> Result<Self, Error> {
        let lookup = |action_label, script_label| -> Result<_, Error> {
            let action = action_label_map
                .get(action_label)
                .ok_or_else(|| Error::msg("action is undefined"))?;
            let script = script_label_map
                .get(script_label)
                .ok_or_else(|| Error::msg("script is undefined"))?;

            Ok((action, script))
        };
        let inner = action_script_pairs
            .map(|(action_label, script_label)| {
                lookup(action_label, script_label).with_context(|| {
                    format!(
                        "while processing action-script pair(\"{}\": \"{}\"): action is undefined",
                        action_label, script_label,
                    )
                })
            })
            .collect::<Result<IntMap<_, _>, Error>>()?;

        Ok(Self(inner))
    }

    pub fn get(&self, action: &Action) -> Option<&Script> {
        self.0.get(action)
    }
}
