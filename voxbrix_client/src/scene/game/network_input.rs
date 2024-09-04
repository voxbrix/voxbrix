use crate::{
    component::actor::TargetQueue,
    scene::game::{
        GameSharedData,
        Transition,
    },
};
use log::error;
use std::time::Instant;
use voxbrix_common::{
    component::{
        actor::{
            orientation::Orientation,
            position::Position,
        },
        chunk::status::ChunkStatus,
    },
    entity::actor::Actor,
    messages::client::ClientAccept,
    pack,
    ChunkData,
};
use voxbrix_protocol::client::Error as ClientError;

pub struct NetworkInput<'a> {
    pub shared_data: &'a mut GameSharedData,
    pub event: Result<Vec<u8>, ClientError>,
}

impl NetworkInput<'_> {
    pub fn run(self) -> Transition {
        let NetworkInput {
            shared_data: sd,
            event,
        } = self;

        let message = match event {
            Ok(m) => m,
            Err(err) => {
                // TODO handle properly, pass error to menu to display there
                error!("game::run: connection error: {:?}", err);
                return Transition::Menu;
            },
        };

        let message = match sd.packer.unpack::<ClientAccept>(&message) {
            Ok(m) => m,
            Err(_) => return Transition::None,
        };

        match message {
            ClientAccept::State {
                snapshot: new_lss,
                last_client_snapshot: new_lcs,
                state,
                actions,
            } => {
                let current_time = Instant::now();

                let Ok(state) = sd.state_unpacker.unpack_state(state) else {
                    return Transition::None;
                };

                sd.class_ac.unpack_state(&state);
                sd.model_acc.unpack_state(&state);
                sd.velocity_ac.unpack_state(&state);
                sd.target_orientation_ac.unpack_state_convert(
                    &state,
                    |actor, previous, orientation: Orientation| {
                        let current_value = if let Some(p) = sd.orientation_ac.get(&actor) {
                            *p
                        } else {
                            sd.orientation_ac.insert(actor, orientation, sd.snapshot);
                            orientation
                        };

                        TargetQueue::from_previous(
                            previous,
                            current_value,
                            orientation,
                            current_time,
                            new_lss,
                        )
                    },
                );
                sd.target_position_ac.unpack_state_convert(
                    &state,
                    |actor, previous, position: Position| {
                        let current_value = if let Some(p) = sd.position_ac.get(&actor) {
                            *p
                        } else {
                            sd.position_ac.insert(actor, position, sd.snapshot);
                            position
                        };

                        TargetQueue::from_previous(
                            previous,
                            current_value,
                            position,
                            current_time,
                            new_lss,
                        )
                    },
                );

                sd.actions_packer.confirm_snapshot(new_lcs);

                let actions = match sd.actions_unpacker.unpack_actions(actions) {
                    Ok(m) => m,
                    Err(_) => return Transition::Menu,
                };

                // Filtering out already handled actions
                for (action, _, data) in actions
                    .data()
                    .iter()
                    .filter(|(_, snapshot, _)| *snapshot > sd.last_server_snapshot)
                {
                    let (actor_opt, action_data): (Option<Actor>, &[u8]) =
                        pack::decode_from_slice(data)
                            .expect("unable to unpack server answer")
                            .0;
                    error!(
                        "received action {:?} of {:?} with data len {}",
                        action,
                        actor_opt,
                        action_data.len()
                    );
                }

                sd.last_client_snapshot = new_lcs;
                sd.last_server_snapshot = new_lss;
            },
            ClientAccept::ChunkData(ChunkData {
                chunk,
                block_classes,
            }) => {
                sd.class_bc.insert_chunk(chunk, block_classes);
                sd.status_cc.insert(chunk, ChunkStatus::Active);

                sd.sky_light_system.enqueue_chunk(chunk);
            },
            ClientAccept::ChunkChanges(changes) => {
                let Ok(mut chunk_decoder) = changes.decode_chunks() else {
                    error!("unable to decode chunk changes");
                    return Transition::Menu;
                };

                while let Some(chunk_change) = chunk_decoder.decode_chunk() {
                    let Ok(mut chunk_change) = chunk_change else {
                        error!("unable to decode chunk change");
                        return Transition::Menu;
                    };

                    let chunk = chunk_change.chunk();

                    let mut chunk_classes = sd.class_bc.get_mut_chunk(&chunk);

                    while let Some(block_change) = chunk_change.decode_block() {
                        let Ok((block, block_class)) = block_change else {
                            error!("unable to decode block changes");
                            return Transition::Menu;
                        };

                        if let Some(ref mut chunk_classes) = chunk_classes {
                            *chunk_classes.get_mut(block) = block_class;
                            sd.sky_light_system.block_change(&chunk, block);
                            sd.block_render_system.block_change(&chunk, block);
                        }
                    }
                }
            },
        }

        Transition::None
    }
}
