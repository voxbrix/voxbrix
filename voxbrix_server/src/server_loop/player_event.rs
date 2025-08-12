use crate::{
    component::player::{
        client::ClientPlayerComponent,
        dispatches_packer::DispatchesPackerPlayerComponent,
    },
    entity::player::Player,
    system::{
        player_actions::PlayerActionsSystem,
        player_updates::PlayerUpdatesSystem,
    },
};
use log::debug;
use std::mem;
use voxbrix_common::{
    messages::server::ServerAccept,
    pack::Packer,
    resource::removal_queue::RemovalQueue,
};
use voxbrix_protocol::server::ReceivedData;
use voxbrix_world::World;

pub struct PlayerEvent<'a> {
    pub world: &'a mut World,
    pub player: Player,
    pub message: ReceivedData,
}

impl PlayerEvent<'_> {
    // If this fails we drop the player.
    fn try_run(&mut self, packer: &mut Packer) -> Result<(), ()> {
        let Self {
            world,
            player,
            message,
        } = self;

        let event = packer
            .unpack::<ServerAccept>(message.data().as_ref())
            .map_err(|_| {
                debug!(
                    "server_loop: unable to parse data from player {:?} on base channel",
                    player
                );
            })?;

        match event {
            ServerAccept::State(state) => {
                world
                    .get_data::<PlayerUpdatesSystem>()
                    .run(*player, &state)
                    .map_err(|_| {
                        debug!(
                            "server_loop: unable to parse updates from player {:?}",
                            player
                        );
                    })?;

                world
                    .get_data::<PlayerActionsSystem>()
                    .run(*player, &state)
                    .map_err(|_| {
                        debug!(
                            "server_loop: unable to parse actions from player {:?}",
                            player
                        );
                    })?;

                let client = world
                    .get_resource_mut::<ClientPlayerComponent>()
                    .get_mut(&player);

                if let Some(client) = client {
                    client.last_server_snapshot = state.last_server_snapshot;
                    client.last_client_snapshot = state.snapshot;
                }

                let dispatches_packer = world
                    .get_resource_mut::<DispatchesPackerPlayerComponent>()
                    .get_mut(&player);

                if let Some(dispatches_packer) = dispatches_packer {
                    // Pruning confirmed Server -> Client actions.
                    dispatches_packer.confirm_snapshot(state.last_server_snapshot);
                }
            },
        }

        Ok(())
    }

    pub fn run(&mut self) {
        let mut packer = mem::take(self.world.get_resource_mut::<Packer>());

        if self.try_run(&mut packer).is_err() {
            self.world.get_resource_mut::<RemovalQueue<Player>>();
        }

        *self.world.get_resource_mut::<Packer>() = packer;
    }
}
