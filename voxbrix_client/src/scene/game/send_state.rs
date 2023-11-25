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
            .pack_player(&mut sd.client_state, sd.last_client_snapshot);
        sd.velocity_ac
            .pack_player(&mut sd.client_state, sd.last_client_snapshot);
        sd.orientation_ac
            .pack_player(&mut sd.client_state, sd.last_client_snapshot);

        let packed = ServerAccept::pack_state(
            sd.snapshot,
            sd.last_server_snapshot,
            &mut sd.client_state,
            &mut sd.packer,
        );

        let _ = sd.unreliable_tx.send(packed);

        sd.snapshot = sd.snapshot.next();

        Transition::None
    }
}
