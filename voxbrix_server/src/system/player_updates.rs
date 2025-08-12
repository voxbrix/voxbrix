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
use log::{
    debug,
    error,
};
use voxbrix_common::{
    entity::snapshot::ServerSnapshot,
    messages::{
        server::ClientState,
        UpdatesUnpacker,
    },
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct PlayerUpdatesSystem;

impl System for PlayerUpdatesSystem {
    type Data<'a> = PlayerUpdatesSystemData<'a>;
}

pub enum Error {
    Corrupted,
    PlayerActorMissing,
    PlayerHasNoClient,
}

#[derive(SystemData)]
pub struct PlayerUpdatesSystemData<'a> {
    snapshot: &'a ServerSnapshot,
    actor_pc: &'a ActorPlayerComponent,
    client_pc: &'a mut ClientPlayerComponent,
    chunk_update_pc: &'a mut ChunkUpdatePlayerComponent,
    chunk_view_pc: &'a mut ChunkViewPlayerComponent,
    updates_unpacker: &'a mut UpdatesUnpacker,

    position_ac: &'a mut PositionActorComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    orientation_ac: &'a mut OrientationActorComponent,
}

impl PlayerUpdatesSystemData<'_> {
    pub fn run(self, player: Player, state: &ClientState) -> Result<(), Error> {
        let updates = self.updates_unpacker.unpack(&state.updates).map_err(|_| {
            debug!("skipping corrupted updates");

            Error::Corrupted
        })?;

        // TODO: there could be sequential messaged on the wire that have the same state
        // changes duplicated. Make sure that those do not duplicate in the components.
        let actor = self.actor_pc.get(&player).ok_or_else(|| {
            error!("player has no actor");

            Error::PlayerActorMissing
        })?;

        let client = self.client_pc.get_mut(&player).ok_or_else(|| {
            error!("player has no client");

            Error::PlayerHasNoClient
        })?;

        if client.last_client_snapshot >= state.snapshot {
            return Ok(());
        }

        self.velocity_ac
            .unpack_player(actor, &updates, *self.snapshot);
        self.orientation_ac
            .unpack_player(actor, &updates, *self.snapshot);

        self.position_ac.unpack_player_with(
            actor,
            &updates,
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

        Ok(())
    }
}
