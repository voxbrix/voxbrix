use crate::{
    component::actor::{
        orientation::OrientationActorComponent,
        position::PositionActorComponent,
        velocity::VelocityActorComponent,
    },
    resource::{
        confirmed_snapshots::ConfirmedSnapshots,
        player_chunk_view_radius::PlayerChunkViewRadius,
        server_sender::ServerSender,
    },
};
use voxbrix_common::{
    entity::{
        actor::Actor,
        snapshot::Snapshot,
    },
    messages::{
        server::{
            ClientState,
            ServerAccept,
        },
        ActionsPacker,
        StatePacker,
    },
    pack::Packer,
    resource::removal_queue::RemovalQueue,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct SendChangesSystem;

impl System for SendChangesSystem {
    type Data<'a> = SendChangesSystemData<'a>;
}

#[derive(SystemData)]
pub struct SendChangesSystemData<'a> {
    snapshot: &'a mut Snapshot,
    position_ac: &'a mut PositionActorComponent,
    orientation_ac: &'a mut OrientationActorComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    packer: &'a mut Packer,
    state_packer: &'a mut StatePacker,
    actions_packer: &'a mut ActionsPacker,
    player_chunk_view_radius: &'a PlayerChunkViewRadius,
    confirmed_snapshots: &'a ConfirmedSnapshots,
    server_sender: &'a ServerSender,
    actor_rq: &'a mut RemovalQueue<Actor>,
}

impl SendChangesSystemData<'_> {
    pub fn run(self) {
        // Removing out-of-bounds actors
        let inactive_actors = self
            .position_ac
            .iter()
            .filter(|(_, position)| {
                self.position_ac
                    .player_chunks()
                    .find(|player_chunk| {
                        player_chunk
                            .radius(self.player_chunk_view_radius.0)
                            .is_within(&position.chunk)
                    })
                    .is_none()
            })
            .map(|(actor, _)| actor);

        // TODO Not deleted instantly, must be very careful with the edge cases.
        for actor in inactive_actors {
            self.actor_rq.enqueue(actor);
        }

        let lcs = self.confirmed_snapshots.last_client_snapshot;

        self.position_ac.pack_player(self.state_packer, lcs);
        self.velocity_ac.pack_player(self.state_packer, lcs);
        self.orientation_ac.pack_player(self.state_packer, lcs);

        let packed = self.packer.pack_to_vec(&ServerAccept::State(ClientState {
            snapshot: *self.snapshot,
            last_server_snapshot: self.confirmed_snapshots.last_server_snapshot,
            state: self.state_packer.pack_state(),
            actions: self.actions_packer.pack_actions(),
        }));

        let _ = self.server_sender.unreliable.send(packed);

        *self.snapshot = self.snapshot.next();
    }
}
