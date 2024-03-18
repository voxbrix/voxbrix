use crate::scene::game::{
    GameSharedData,
    Transition,
};
use voxbrix_common::messages::server::ServerAccept;

pub struct SendState<'a> {
    pub shared_data: &'a mut GameSharedData,
}

impl SendState<'_> {
    pub fn run(self) -> Transition {
        let SendState { shared_data: sd } = self;

        sd.position_ac
            .pack_player(&mut sd.state_packer, sd.last_client_snapshot);
        sd.velocity_ac
            .pack_player(&mut sd.state_packer, sd.last_client_snapshot);
        sd.orientation_ac
            .pack_player(&mut sd.state_packer, sd.last_client_snapshot);

        let packed = sd.packer.pack_to_vec(&ServerAccept::State {
            snapshot: sd.snapshot,
            last_server_snapshot: sd.last_server_snapshot,
            state: sd.state_packer.pack_state(),
            actions: sd.actions_packer.pack_actions(),
        });

        let _ = sd.unreliable_tx.send(packed);

        sd.snapshot = sd.snapshot.next();

        Transition::None
    }
}
