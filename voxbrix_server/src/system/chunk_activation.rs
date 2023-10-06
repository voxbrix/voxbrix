use crate::{
    component::{
        actor::{
            chunk_activation::{
                ActorChunkActivation,
                ChunkActivationActorComponent,
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
    },
    storage::IntoDataSized,
    Shared,
    BLOCK_CLASS_TABLE,
};
use ahash::AHashSet;
use redb::ReadableTable;
use tokio::task;
use voxbrix_common::{
    component::block::{
        class::ClassBlockComponent,
        BlocksVec,
    },
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
    target: AHashSet<Chunk>,
    missing: Vec<Chunk>,
}

impl ChunkActivationSystem {
    pub fn new() -> Self {
        Self {
            target: AHashSet::new(),
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
            .flat_map(|(chunk, chunk_activation)| {
                let ActorChunkActivation { radius } = chunk_activation;
                let radius = chunk.radius(*radius);
                radius.into_iter()
            });

        self.target.extend(iter);
    }

    pub fn apply(
        &mut self,
        shared: &'static Shared,
        status_cc: &mut StatusChunkComponent,
        class_bc: &mut ClassBlockComponent,
        cache_cc: &mut CacheChunkComponent,
        send_fn: impl Fn(Chunk, ChunkActivationOutcome, &mut Packer) + Clone + Send + 'static,
    ) {
        self.missing.clear();
        self.missing.extend(
            self.target
                .iter()
                .filter(|chunk| {
                    let is_new = status_cc.get(chunk).is_none();

                    if is_new {
                        status_cc.insert(**chunk, ChunkStatus::Loading);
                    }

                    is_new
                })
                .copied(),
        );

        // TODO: sort new_chunks by the sum of distances to the actors

        status_cc
            .retain(|chunk, status| self.target.contains(chunk) || *status == ChunkStatus::Loading);
        cache_cc.retain(|chunk, _| self.target.contains(chunk));
        class_bc.retain(|chunk| self.target.contains(chunk));

        for chunk in self.missing.iter().copied() {
            let send_fn = send_fn.clone();
            task::spawn_blocking(move || {
                let mut packer = Packer::new();

                let db_read = shared.database.begin_read().unwrap();
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
}
