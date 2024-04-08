use crate::{
    component::{
        actor::{
            chunk_activation::{
                ActorChunkActivation,
                ChunkActivationActorComponent,
            },
            position::PositionActorComponent,
        },
        chunk::status::{
            ChunkStatus,
            StatusChunkComponent,
        },
    },
    storage::IntoDataSized,
    BLOCK_CLASS_TABLE,
};
use ahash::AHashMap;
use redb::{
    Database,
    ReadableTable,
};
use std::sync::Arc;
use tokio::runtime::Handle;
use voxbrix_common::{
    component::block::BlocksVec,
    entity::{
        block_class::BlockClass,
        chunk::Chunk,
    },
    pack::Packer,
};

pub enum ChunkActivationOutcome {
    ChunkActivated(BlocksVec<BlockClass>),
    ChunkNeedsGeneration,
}

pub struct ChunkActivationSystem {
    target: AHashMap<Chunk, f64>,
    missing: Vec<(Chunk, f64)>,
}

impl ChunkActivationSystem {
    pub fn new() -> Self {
        Self {
            target: AHashMap::new(),
            missing: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.target.clear();
        self.missing.clear();
    }

    pub fn actor_activations(
        &mut self,
        chunk_activation_ac: &ChunkActivationActorComponent,
        position_ac: &PositionActorComponent,
    ) {
        let iter = chunk_activation_ac
            .iter()
            .filter_map(|(actor, chunk_activation)| {
                Some((position_ac.get(&actor)?.chunk, chunk_activation))
            })
            .flat_map(|(actor_chunk, chunk_activation)| {
                let ActorChunkActivation { radius } = chunk_activation;
                let chunk_radius = actor_chunk.radius(*radius);
                chunk_radius.into_iter_simple().map(move |chunk| {
                    let reverse_priority: f64 = actor_chunk
                        .position
                        .iter()
                        .zip(chunk.position.iter())
                        .map(|(actor_coord, chunk_coord)| {
                            ((chunk_coord - actor_coord) as f64 + 0.5).abs().powi(2)
                        })
                        .sum();

                    let priority = 1.0 - reverse_priority.sqrt();

                    (chunk, priority)
                })
            });

        for (chunk, priority) in iter {
            if let Some(existing_priority) = self.target.get_mut(&chunk) {
                *existing_priority += priority;
            } else {
                self.target.insert(chunk, priority);
            }
        }
    }

    pub fn activate(
        &mut self,
        database: &Arc<Database>,
        status_cc: &mut StatusChunkComponent,
        send_fn: impl Fn(Chunk, ChunkActivationOutcome, &mut Packer) + Clone + Send + 'static,
        rt_handle: &Handle,
    ) {
        self.missing.clear();
        self.missing.extend(
            self.target
                .iter()
                .filter(|(chunk, _)| {
                    let is_new = status_cc.get(chunk).is_none();

                    if is_new {
                        status_cc.insert(**chunk, ChunkStatus::Loading);
                    }

                    is_new
                })
                .map(|(chunk, priority)| (*chunk, *priority)),
        );

        self.missing
            .sort_unstable_by(|(_, priority1), (_, priority2)| {
                priority2.partial_cmp(priority1).unwrap()
            });

        for (chunk, _) in self.missing.iter().copied() {
            let send_fn = send_fn.clone();
            let database = database.clone();
            rt_handle.spawn_blocking(move || {
                let mut packer = Packer::new();

                let db_read = database.begin_read().unwrap();
                let table = db_read
                    .open_table(BLOCK_CLASS_TABLE)
                    .expect("server_loop: database read");

                let block_classes = table
                    .get(chunk.into_data_sized())
                    .unwrap()
                    .map(|bytes| bytes.value().into_inner(&mut packer));

                if let Some(block_classes) = block_classes {
                    send_fn(
                        chunk,
                        ChunkActivationOutcome::ChunkActivated(block_classes),
                        &mut packer,
                    );
                } else {
                    send_fn(
                        chunk,
                        ChunkActivationOutcome::ChunkNeedsGeneration,
                        &mut packer,
                    );
                }
            });
        }
    }

    pub fn is_active(&self, chunk: &Chunk) -> bool {
        self.target.contains_key(chunk)
    }
}
