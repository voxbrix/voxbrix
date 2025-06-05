use crate::component::{
    actor::{
        player::PlayerActorComponent,
        position::PositionActorComponent,
    },
    chunk::status::{
        ChunkStatus,
        StatusChunkComponent,
    },
};
use voxbrix_common::{
    entity::{
        actor::Actor,
        snapshot::Snapshot,
    },
    resource::removal_queue::RemovalQueue,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct ActorPruningSystem;

impl System for ActorPruningSystem {
    type Data<'a> = ActorPruningSystemData<'a>;
}

#[derive(SystemData)]
pub struct ActorPruningSystemData<'a> {
    snapshot: &'a Snapshot,

    player_ac: &'a PlayerActorComponent,
    position_ac: &'a PositionActorComponent,
    removal_queue_ac: &'a mut RemovalQueue<Actor>,

    statuc_cc: &'a StatusChunkComponent,
}

impl ActorPruningSystemData<'_> {
    // Removing non-player actors that are now on inactive (nonexistent) chunk.
    pub fn run(self) {
        for actor in self
            .position_ac
            .actors_chunk_changes()
            // Reverting because the original order is "old snapshot to new snapshot".
            // We need only the last snapshot.
            .rev()
            .take_while(|change| &change.snapshot == self.snapshot)
            .map(|change| change.actor)
            .filter_map(|actor| {
                // Nonexistent position should be impossible in this case
                let pos = self.position_ac.get(&actor)?;

                // Ignoring player actors to avoid bugs
                if self.player_ac.get(&actor).is_some() {
                    return None;
                }

                match self.statuc_cc.get(&pos.chunk) {
                    Some(ChunkStatus::Active) => None,
                    None | Some(ChunkStatus::Loading) => Some(actor),
                }
            })
        {
            self.removal_queue_ac.enqueue(actor);
        }
    }
}
