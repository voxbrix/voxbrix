use crate::{
    component::player::{
        actions_packer::ActionsPackerPlayerComponent,
        client::ClientPlayerComponent,
    },
    entity::player::Player,
    system::{
        player_actions::PlayerActionsSystem,
        player_state::PlayerStateSystem,
    },
};
use log::debug;
use std::mem;
use voxbrix_common::{
    messages::server::ServerAccept,
    pack::Packer,
};
use voxbrix_protocol::server::ReceivedData;
use voxbrix_world::World;

pub struct PlayerEvent<'a> {
    pub world: &'a mut World,
    pub player: Player,
    pub message: ReceivedData,
}

impl PlayerEvent<'_> {
    pub fn run(self) {
        let Self {
            world,
            player,
            message,
        } = self;

        let mut packer = mem::take(world.get_resource_mut::<Packer>());

        match packer.unpack::<ServerAccept>(message.data().as_ref()) {
            Ok(event) => {
                match event {
                    ServerAccept::State(state) => {
                        world.get_data::<PlayerStateSystem>().run(player, &state);

                        world.get_data::<PlayerActionsSystem>().run(player, &state);

                        let client = world
                            .get_resource_mut::<ClientPlayerComponent>()
                            .get_mut(&player);

                        if let Some(client) = client {
                            client.last_server_snapshot = state.last_server_snapshot;
                            client.last_client_snapshot = state.snapshot;
                        }

                        let actions_packer = world
                            .get_resource_mut::<ActionsPackerPlayerComponent>()
                            .get_mut(&player);

                        if let Some(actions_packer) = actions_packer {
                            // Pruning confirmed Server -> Client actions.
                            actions_packer.confirm_snapshot(state.last_server_snapshot);
                        }
                    },
                }
            },
            Err(_) => {
                debug!(
                    "server_loop: unable to parse data from player {:?} on base channel",
                    player
                );
            },
        }

        *world.get_resource_mut::<Packer>() = packer;
    }
}
