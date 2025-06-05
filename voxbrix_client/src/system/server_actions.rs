use crate::resource::confirmed_snapshots::ConfirmedSnapshots;
use log::error;
use voxbrix_common::{
    entity::actor::Actor,
    messages::{
        client::ServerState,
        ActionsUnpacker,
    },
    pack,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub enum Error {
    /// Unable to unpack state.
    UnpackError,
}

pub struct ServerActionsSystem;

impl System for ServerActionsSystem {
    type Data<'a> = ServerActionsSystemData<'a>;
}

#[derive(SystemData)]
pub struct ServerActionsSystemData<'a> {
    confirmed_snapshots: &'a ConfirmedSnapshots,
    actions_unpacker: &'a mut ActionsUnpacker,
}

impl ServerActionsSystemData<'_> {
    pub fn run(&mut self, data: &ServerState) -> Result<(), Error> {
        let actions = self
            .actions_unpacker
            .unpack_actions(&data.actions)
            .map_err(|_| Error::UnpackError)?;

        // Filtering out already handled actions
        for (action, _, data) in actions
            .data()
            .iter()
            .filter(|(_, snapshot, _)| *snapshot > self.confirmed_snapshots.last_server_snapshot)
        {
            let (actor_opt, action_data): (Option<Actor>, &[u8]) = pack::decode_from_slice(data)
                .expect("unable to unpack server answer")
                .0;
            error!(
                "received action {:?} of {:?} with data len {}",
                action,
                actor_opt,
                action_data.len()
            );
        }

        Ok(())
    }
}
