use crate::{
    component::{
        actor::{
            chunk_activation::{
                ActorChunkPruning,
                ChunkPruningActorComponent,
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
    resource::{
        shared_event::SharedEvent,
        chunk_generation_request::ChunkGenerationRequest,
    },
};
use ahash::AHashMap;
use redb::Database;
use std::sync::Arc;
use tokio::runtime::Handle;
use voxbrix_common::{
    component::block::BlocksVec,
    entity::{
        block_class::BlockClass,
        chunk::Chunk,
    },
    messages::client::ClientAccept,
    pack::Packer,
    ChunkData,
};
use voxbrix_world::{System, SystemData};
use flume::Sender;

pub enum ChunkPruningOutcome {
    ChunkActivated(BlocksVec<BlockClass>),
    ChunkNeedsGeneration,
}

#[derive(SystemData)]
pub struct ChunkPruningSystemData<'a> {
    system: &'a mut ChunkPruningSystem,
    chunk_activation_ac: &'a ChunkPruningActorComponent,
    position_ac: &'a PositionActorComponent,
    database: &'a Arc<Database>,
    status_cc: &'a mut StatusChunkComponent,
    rt_handle: &'a Handle,
    shared_event_tx: &'a Sender<SharedEvent>,
    chunk_generation_tx: &'a Sender<ChunkGenerationRequest>,
}

pub struct ChunkPruningSystem {
    target: AHashMap<Chunk, f64>,
    missing: Vec<(Chunk, f64)>,
}

impl System for ChunkPruningSystem {
    type Data<'a> = ChunkPruningSystemData<'a>;
}

impl ChunkPruningSystem {
    pub fn new() -> Self {
        Self {
            target: AHashMap::new(),
            missing: Vec::new(),
        }
    }
}

impl ChunkPruningSystemData<'_> {
    pub fn run(self) {
        // Calculating target:
        self.system.target.clear();
        let iter = self.chunk_activation_ac
            .iter()
            .filter_map(|(actor, chunk_activation)| {
                Some((self.position_ac.get(&actor)?.chunk, chunk_activation))
            })
            .flat_map(|(actor_chunk, chunk_activation)| {
                let ActorChunkPruning { radius } = chunk_activation;
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
            if let Some(existing_priority) = self.system.target.get_mut(&chunk) {
                *existing_priority += priority;
            } else {
                self.system.target.insert(chunk, priority);
            }
        }

        // Activation:
        self.system.missing.clear();
        self.system.missing.extend(
            self.system.target
                .iter()
                .filter(|(chunk, _)| {
                    let is_new = self.status_cc.get(chunk).is_none();

                    if is_new {
                        self.status_cc.insert(**chunk, ChunkStatus::Loading);
                    }

                    is_new
                })
                .map(|(chunk, priority)| (*chunk, *priority)),
        );

        self.system.missing
            .sort_unstable_by(|(_, priority1), (_, priority2)| {
                priority2.partial_cmp(priority1).unwrap()
            });

        for (chunk, _) in self.system.missing.iter().copied() {
            let shared_event_tx = self.shared_event_tx.clone();
            let chunk_generation_tx = self.chunk_generation_tx.clone();
            let database = self.database.clone();
            self.rt_handle.spawn_blocking(move || {
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
                    let data = ChunkData {
                        chunk,
                        block_classes,
                    };

                    let data_encoded =packer.pack_to_vec(&ClientAccept::ChunkData(data.clone())).into();

                    let _ =
                        shared_event_tx.send(SharedEvent::ChunkLoaded { data, data_encoded });
                } else {
                    let _ = chunk_generation_tx.send(ChunkGenerationRequest { chunk });
                }
            });
        }
    }
}
