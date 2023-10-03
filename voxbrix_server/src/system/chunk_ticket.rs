use crate::component::{
    actor::{
        chunk_ticket::{
            ActorChunkTicket,
            ChunkTicketActorComponent,
        },
        position::PositionActorComponent,
    },
    chunk::{
        cache::CacheChunkComponent,
        status::{
            ChunkStatus,
            StatusChunkComponent,
        },
    },
};
use std::collections::BTreeSet;
use tokio::task;
use voxbrix_common::{
    component::block::class::ClassBlockComponent,
    entity::chunk::Chunk,
};

pub struct ChunkTicketSystem {
    data: BTreeSet<Chunk>,
}

impl ChunkTicketSystem {
    pub fn new() -> Self {
        Self {
            data: BTreeSet::new(),
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn actor_tickets(
        &mut self,
        chunk_ticket_ac: &ChunkTicketActorComponent,
        position_ac: &PositionActorComponent,
    ) {
        let iter = chunk_ticket_ac
            .iter()
            .filter_map(|(actor, chunk_ticket)| {
                Some((position_ac.get(&actor)?.chunk, chunk_ticket))
            })
            .flat_map(|(chunk, chunk_ticket)| {
                let ActorChunkTicket { radius } = chunk_ticket;
                let radius = chunk.radius(*radius);
                radius.into_iter()
            });

        self.data.extend(iter);
    }

    pub fn apply<F>(
        &self,
        status_cc: &mut StatusChunkComponent,
        class_bc: &mut ClassBlockComponent,
        cache_cc: &mut CacheChunkComponent,
        mut f: F,
    ) where
        F: 'static + FnMut(&Chunk) + Send,
    {
        let new_chunks = self
            .data
            .iter()
            .filter(|chunk| {
                let is_new = status_cc.get(chunk).is_none();
                if is_new {
                    status_cc.insert(**chunk, ChunkStatus::Loading);
                }
                is_new
            })
            .cloned()
            .collect::<Vec<_>>();

        // TODO: sort new_chunks by the sum of distances to the actors

        status_cc
            .retain(|chunk, status| self.data.contains(chunk) || *status == ChunkStatus::Loading);
        cache_cc.retain(|chunk, _| self.data.contains(chunk));
        class_bc.retain(|chunk| self.data.contains(chunk));

        task::spawn_blocking(move || {
            for chunk in new_chunks {
                f(&chunk);
            }
        });
    }
}
