use crate::resource::confirmed_snapshots::ConfirmedSnapshots;
use log::error;
use voxbrix_common::{
    entity::actor::Actor,
    messages::{
        client::ServerState,
        DispatchesUnpacker,
    },
    pack,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub enum Error {
    /// Unable to unpack dispatches.
    UnpackError,
}

pub struct ServerDispatchesSystem;

impl System for ServerDispatchesSystem {
    type Data<'a> = ServerDispatchesSystemData<'a>;
}

#[derive(SystemData)]
pub struct ServerDispatchesSystemData<'a> {
    confirmed_snapshots: &'a ConfirmedSnapshots,
    dispatches_unpacker: &'a mut DispatchesUnpacker,
}

impl ServerDispatchesSystemData<'_> {
    pub fn run(&mut self, data: &ServerState) -> Result<(), Error> {
        let dispatches = self
            .dispatches_unpacker
            .unpack(&data.dispatches)
            .map_err(|_| Error::UnpackError)?;

        // Filtering out already handled dispatches
        for (dispatch, _, data) in dispatches
            .data()
            .iter()
            .filter(|(_, snapshot, _)| *snapshot > self.confirmed_snapshots.last_server_snapshot)
        {
            let (actor_opt, dispatch_data): (Actor, &[u8]) = pack::decode_from_slice(data)
                .expect("unable to unpack server answer")
                .0;
            error!(
                "received dispatch {:?} of {:?} with data len {}",
                dispatch,
                actor_opt,
                dispatch_data.len()
            );
        }

        Ok(())
    }
}
