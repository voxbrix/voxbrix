use crate::{
    component::{
        actor::{
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
        },
        player::{
            actor::ActorPlayerComponent,
            chunk_update::{
                ChunkUpdate,
                ChunkUpdatePlayerComponent,
                FullChunkView,
            },
            chunk_view::ChunkViewPlayerComponent,
            client::ClientPlayerComponent,
        },
    },
    entity::player::Player,
};
use log::debug;
use voxbrix_common::{
    entity::snapshot::Snapshot,
    messages::{
        server::ClientState,
        StateUnpacker,
    },
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct PlayerStateSystem;

impl System for PlayerStateSystem {
    type Data<'a> = PlayerStateSystemData<'a>;
}

#[derive(SystemData)]
pub struct PlayerStateSystemData<'a> {
    snapshot: &'a Snapshot,
    actor_pc: &'a ActorPlayerComponent,
    client_pc: &'a mut ClientPlayerComponent,
    chunk_update_pc: &'a mut ChunkUpdatePlayerComponent,
    chunk_view_pc: &'a mut ChunkViewPlayerComponent,
    state_unpacker: &'a mut StateUnpacker,

    position_ac: &'a mut PositionActorComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    orientation_ac: &'a mut OrientationActorComponent,
}

impl PlayerStateSystemData<'_> {
    pub fn run(self, player: Player, state_change: &ClientState) {
        let state = match self.state_unpacker.unpack_state(&state_change.state) {
            Ok(v) => v,
            Err(_) => {
                debug!("skipping corrupted state");
                return;
            },
        };

        // TODO: there could be sequential messaged on the wire that have the same state
        // changes duplicated. Make sure that those do not duplicate in the components.
        let actor = match self.actor_pc.get(&player) {
            Some(a) => a,
            None => return,
        };

        let client = match self.client_pc.get_mut(&player) {
            Some(c) => c,
            None => return,
        };

        if client.last_client_snapshot >= state_change.snapshot {
            return;
        }

        self.velocity_ac
            .unpack_player(actor, &state, *self.snapshot);
        self.orientation_ac
            .unpack_player(actor, &state, *self.snapshot);

        self.position_ac.unpack_player_with(
            actor,
            &state,
            *self.snapshot,
            |old_value, new_value| {
                let chunk = match new_value {
                    Some(v) => v.chunk,
                    None => return,
                };

                self.client_pc
                    .get_mut(&player)
                    .unwrap()
                    .last_confirmed_chunk = Some(chunk);

                if old_value.is_none() || old_value.is_some() && old_value.unwrap().chunk != chunk {
                    let prev_view_radius = match self.chunk_view_pc.get(&player) {
                        Some(r) => r.radius,
                        None => return,
                    };

                    let previous_view = old_value.map(|old_pos| {
                        FullChunkView {
                            chunk: old_pos.chunk,
                            radius: prev_view_radius,
                        }
                    });

                    if self.chunk_update_pc.get(&player).is_some() {
                        return;
                    } else {
                        self.chunk_update_pc
                            .insert(player, ChunkUpdate { previous_view });
                    }
                }
            },
        );
    }
}
