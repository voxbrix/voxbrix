use crate::{
    component::{
        block::class::ClassBlockComponent,
        chunk::sky_light_data::SkyLightDataChunkComponent,
    },
    resource::confirmed_snapshots::ConfirmedSnapshots,
    scene::game::Transition,
    system::{
        chunk_changes_accept::ChunkChangesAcceptSystem,
        server_dispatches::ServerDispatchesSystem,
        server_updates::ServerUpdatesSystem,
    },
};
use log::error;
use voxbrix_common::{
    component::chunk::status::{
        ChunkStatus,
        StatusChunkComponent,
    },
    messages::client::ClientAccept,
    pack::Packer,
    ChunkData,
};
use voxbrix_protocol::client::Error as ClientError;
use voxbrix_world::World;

pub struct NetworkInput<'a> {
    pub world: &'a mut World,
    pub event: Result<Vec<u8>, ClientError>,
}

impl NetworkInput<'_> {
    pub fn run(self) -> Transition {
        let NetworkInput { world, event } = self;

        let message = match event {
            Ok(m) => m,
            Err(err) => {
                // TODO handle properly, pass error to menu to display there
                error!("game::run: connection error: {:?}", err);
                return Transition::Menu;
            },
        };

        let mut packer = world.take_resource::<Packer>();

        let transition = match packer.unpack::<ClientAccept>(&message) {
            Ok(message) => {
                match message {
                    ClientAccept::State(state) => {
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
                    },
                    ClientAccept::ChunkData(ChunkData {
                        chunk,
                        block_classes,
                    }) => {
                        world
                            .get_resource_mut::<ClassBlockComponent>()
                            .insert_chunk(chunk, block_classes);
                        world
                            .get_resource_mut::<StatusChunkComponent>()
                            .insert(chunk, ChunkStatus::Active);

                        world
                            .get_resource_mut::<SkyLightDataChunkComponent>()
                            .enqueue_chunk(chunk);
                    },
                    ClientAccept::ChunkChanges(changes) => {
                        if world
                            .get_data::<ChunkChangesAcceptSystem>()
                            .run(changes)
                            .is_err()
                        {
                            error!("unable to decode chunk changes");
                            return Transition::Menu;
                        }
                    },
                }

                Transition::None
            },
            Err(_) => Transition::None,
        };

        world.return_resource(packer);

        transition
    }
}
