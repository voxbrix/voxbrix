use voxbrix_common::entity::snapshot::Snapshot;

pub struct ConfirmedSnapshots {
    pub last_client_snapshot: Snapshot,
    pub last_server_snapshot: Snapshot,
}
