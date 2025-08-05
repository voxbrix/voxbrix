use std::collections::VecDeque;
use voxbrix_common::entity::{
    actor::Actor,
    snapshot::{
        ServerSnapshot,
        MAX_SNAPSHOT_DIFF,
    },
};

pub struct ActorRegistry {
    next_max_id: u32,
    free_ids: VecDeque<(ServerSnapshot, u32)>,
}

impl ActorRegistry {
    pub fn new() -> Self {
        Self {
            next_max_id: 0,
            free_ids: VecDeque::new(),
        }
    }

    pub fn add(&mut self, snapshot: ServerSnapshot) -> Actor {
        let reuse_id = self
            .free_ids
            .front()
            .map(|(removal_snapshot, _)| {
                let diff = snapshot
                    .0
                    .checked_sub(removal_snapshot.0)
                    .expect("removal of actor happened before adding");

                // Make sure all removals were already propagated
                diff > MAX_SNAPSHOT_DIFF
            })
            .unwrap_or(false);

        let id = if reuse_id {
            let (_, id) = self.free_ids.pop_front().unwrap();
            id
        } else {
            let id = self.next_max_id;
            self.next_max_id += 1;
            id
        };

        Actor(id)
    }

    pub fn remove(&mut self, actor: &Actor, snapshot: ServerSnapshot) {
        self.free_ids.push_back((snapshot, actor.0));
    }
}
