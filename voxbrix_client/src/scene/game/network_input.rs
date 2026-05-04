use crate::{
    resource::confirmed_snapshots::ConfirmedSnapshots,
    scene::game::{
        NetworkError,
        NetworkMessage,
        Transition,
    },
    system::{
        chunk_changes_accept::ChunkChangesAcceptSystem,
        chunk_data_accept::ChunkDataAcceptSystem,
        server_dispatches::ServerDispatchesSystem,
        server_updates::ServerUpdatesSystem,
    },
};
use log::error;
use voxbrix_common::{
    messages::client::ChunkDataDelta,
    pack::Packer,
};
use voxbrix_world::World;

pub struct NetworkInput<'a> {
    pub world: &'a mut World,
    pub event: Result<NetworkMessage, NetworkError>,
}

impl NetworkInput<'_> {
    pub fn run(self) -> Transition {
        let NetworkInput { world, event } = self;

        let message = match event {
            Ok(m) => m,
            Err(err) => {
                error!("game::run: network error: {:?}", err);
                return Transition::Menu;
            },
        };

        let mut packer = world.take_resource::<Packer>();

        let transition = match message {
            NetworkMessage::State(message) => {
                let state = match message.unpack_state(&mut packer) {
                    Ok(s) => s,
                    Err(_) => {
                        world.return_resource(packer);
                        return Transition::None;
                    },
                };

                if world.get_data::<ServerUpdatesSystem>().run(&state).is_err() {
                    error!("unable to decode server updates");
                    return Transition::Menu;
                }

                if world
                    .get_data::<ServerDispatchesSystem>()
                    .run(&state)
                    .is_err()
                {
                    error!("unable to decode server dispatches");
                    return Transition::Menu;
                }

                let confirmed_snapshots = world.get_resource_mut::<ConfirmedSnapshots>();

                confirmed_snapshots.last_client_snapshot = state.last_client_snapshot;
                confirmed_snapshots.last_server_snapshot = state.snapshot;

                Transition::None
            },
            NetworkMessage::ChunkDataDelta(message) => {
                let ChunkDataDelta {
                    block_class,
                    block_environment,
                    block_metadata,
                } = match message.unpack_chunk_data_delta(&mut packer) {
                    Ok(d) => d,
                    Err(_) => {
                        world.return_resource(packer);
                        return Transition::None;
                    },
                };

                if world
                    .get_data::<ChunkChangesAcceptSystem>()
                    .run(block_class, block_environment, block_metadata)
                    .is_err()
                {
                    error!("unable to decode chunk changes");
                    return Transition::Menu;
                }

                Transition::None
            },
            NetworkMessage::ChunkData(chunk_data_set) => {
                world
                    .get_data::<ChunkDataAcceptSystem>()
                    .run(chunk_data_set);

                Transition::None
            },
        };

        world.return_resource(packer);

        transition
    }
}
