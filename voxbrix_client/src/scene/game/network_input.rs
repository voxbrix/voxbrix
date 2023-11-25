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
    messages::client::ClientAccept,
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
            } => {
                let current_time = Instant::now();
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
                sd.last_client_snapshot = new_lcs;
                sd.last_server_snapshot = new_lss;
            },
            ClientAccept::ChunkData(ChunkData {
                chunk,
                block_classes,
            }) => {
                sd.class_bc.insert_chunk(chunk, block_classes);
                sd.status_cc.insert(chunk, ChunkStatus::Active);

                sd.render_priority_cc.chunk_added(&chunk);
            },
            ClientAccept::AlterBlock {
                chunk,
                block,
                block_class,
            } => {
                if let Some(block_class_ref) =
                    sd.class_bc.get_mut_chunk(&chunk).map(|c| c.get_mut(block))
                {
                    *block_class_ref = block_class;

                    sd.render_priority_cc.chunk_updated(&chunk);
                }
            },
        }

        Transition::None
    }
}
