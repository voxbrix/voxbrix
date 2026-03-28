use crate::{
    component::{
        block::{
            class::ClassBlockComponent,
            environment::EnvironmentBlockComponent,
            metadata::MetadataBlockComponent,
        },
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
    messages::client::{
        ChunkDataDelta,
        ClientAcceptKind,
        ClientAcceptMessage,
    },
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

        let message = match ClientAcceptMessage::from_bytes(message) {
            Ok(m) => m,
            Err(_) => {
                world.return_resource(packer);
                return Transition::None;
            },
        };

        let transition = match message.kind() {
            ClientAcceptKind::State => {
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
            ClientAcceptKind::ChunkData => {
                let chunk_data_set = match message.unpack_chunk_data(&mut packer) {
                    Ok(d) => d,
                    Err(_) => {
                        error!("unable to decode chunk data set");
                        return Transition::Menu;
                    },
                };

                let mut inner_packer = Packer::new();

                for chunk_data_encoded in chunk_data_set {
                    let Ok(chunk_data) =
                        inner_packer.unpack_compressed::<ChunkData>(chunk_data_encoded)
                    else {
                        error!("unable to decode chunk data set");
                        return Transition::Menu;
                    };

                    let ChunkData {
                        chunk,
                        block_classes,
                        block_environment,
                        block_metadata,
                    } = chunk_data;

                    world
                        .get_resource_mut::<ClassBlockComponent>()
                        .insert_chunk(chunk, block_classes);
                    world
                        .get_resource_mut::<EnvironmentBlockComponent>()
                        .insert_chunk(chunk, block_environment);
                    world
                        .get_resource_mut::<MetadataBlockComponent>()
                        .insert_chunk(chunk, block_metadata);
                    world
                        .get_resource_mut::<StatusChunkComponent>()
                        .insert(chunk, ChunkStatus::Active);

                    world
                        .get_resource_mut::<SkyLightDataChunkComponent>()
                        .enqueue_chunk(chunk);
                }

                Transition::None
            },
            ClientAcceptKind::ChunkDataDelta => {
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
        };

        world.return_resource(packer);

        transition
    }
}
