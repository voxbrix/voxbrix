use crate::{
    component::{
        actor::chunk_ticket::{
            ActorChunkTicket,
            ChunkTicketActorComponent,
        },
        block::class::ClassBlockComponent,
        chunk::status::{
            ChunkStatus,
            StatusChunkComponent,
        },
    },
    entity::chunk::Chunk,
};
use std::collections::BTreeSet;

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

    pub fn actor_tickets(&mut self, ctac: &ChunkTicketActorComponent) {
        let iter = ctac
            .iter()
            .map(|(_, chunk_ticket)| {
                let ActorChunkTicket { chunk, radius } = chunk_ticket;
                let radius = *radius as i32;

                (chunk.position[2].saturating_sub(radius)
                    ..= chunk.position[2].saturating_add(radius))
                    .map(move |z| {
                        (chunk.position[1].saturating_sub(radius)
                            ..= chunk.position[1].saturating_add(radius))
                            .map(move |y| {
                                (chunk.position[0].saturating_sub(radius)
                                    ..= chunk.position[0].saturating_add(radius))
                                    .map(move |x| {
                                        Chunk {
                                            position: [x, y, z].into(),
                                            dimension: chunk.dimension,
                                        }
                                    })
                            })
                    })
            })
            .flatten()
            .flatten()
            .flatten();

        self.data.extend(iter);
    }

    pub fn apply<F>(&self, scc: &mut StatusChunkComponent, cbc: &mut ClassBlockComponent, mut f: F)
    where
        F: 'static + FnMut(&Chunk) + Send,
    {
        let new_chunks = self
            .data
            .iter()
            .filter(|chunk| {
                let is_new = scc.get(&chunk).is_none();
                if is_new {
                    scc.insert(**chunk, ChunkStatus::Loading);
                }
                is_new
            })
            .cloned()
            .collect::<Vec<_>>();

        // TODO: sort new_chunks by the sum of distances to the actors

        scc.retain(|chunk, status| self.data.contains(chunk) || *status == ChunkStatus::Loading);
        cbc.retain(|chunk| self.data.contains(chunk));

        blocking::unblock(move || {
            for chunk in new_chunks {
                f(&chunk);
            }
        })
        .detach();
    }
}
