use voxbrix_common::entity::snapshot::{
    ClientSnapshot,
    ServerSnapshot,
};

pub struct ConfirmedSnapshots {
    pub last_client_snapshot: ClientSnapshot,
    pub last_server_snapshot: ServerSnapshot,
}
