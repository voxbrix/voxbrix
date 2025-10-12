use crate::{
    component::{
        actor::position::PositionActorComponent,
        block::class::ClassBlockComponent,
        chunk::cache::{
            CacheChunkComponent,
            ChunkCache,
        },
        player::{
            actor::ActorPlayerComponent,
            chunk_view::ChunkViewPlayerComponent,
            client::{
                ClientEvent,
                ClientPlayerComponent,
                SendData,
            },
        },
    },
    entity::player::Player,
    storage::{
        IntoData,
        IntoDataSized,
        StorageThread,
    },
    Database,
    BLOCK_CLASS_TABLE,
};
use std::sync::Arc;
use voxbrix_common::{
    messages::client::{
        ChunkChanges,
        ClientAccept,
    },
    pack::Packer,
    resource::removal_queue::RemovalQueue,
    ChunkData,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct BlockSyncSystem;

impl System for BlockSyncSystem {
    type Data<'a> = BlockSyncSystemData<'a>;
}

#[derive(SystemData)]
pub struct BlockSyncSystemData<'a> {
    class_bc: &'a mut ClassBlockComponent,
    actor_pc: &'a ActorPlayerComponent,
    chunk_view_pc: &'a ChunkViewPlayerComponent,
    client_pc: &'a ClientPlayerComponent,
    position_ac: &'a PositionActorComponent,
    cache_cc: &'a mut CacheChunkComponent,
    database: &'a Arc<Database>,
    storage: &'a StorageThread,
    player_rq: &'a mut RemovalQueue<Player>,
    packer: &'a mut Packer,
}

impl BlockSyncSystemData<'_> {
    pub fn run(self) {
        for chunk_changes in self.class_bc.changed_chunks() {
            let blocks_cache = self
                .class_bc
                .get_chunk(chunk_changes.chunk)
                .unwrap()
                .clone();

            let cache_data = ClientAccept::ChunkData(ChunkData {
                chunk: *chunk_changes.chunk,
                block_classes: blocks_cache,
            });

            self.cache_cc.insert(
                *chunk_changes.chunk,
                ChunkCache::new(self.packer.pack_to_vec(&cache_data)),
            );

            let blocks_cache = match cache_data {
                ClientAccept::ChunkData(b) => b.block_classes,
                _ => panic!(),
            };

            let database = self.database.clone();

            let chunk_db = *chunk_changes.chunk;

            self.storage.execute(move || {
                let chunk_db = chunk_db.into_data_sized();
                let mut packer = Packer::new();
                let db_write = database.begin_write().unwrap();
                {
                    let mut table = db_write.open_table(BLOCK_CLASS_TABLE).unwrap();

                    table
                        .insert(chunk_db, blocks_cache.into_data(&mut packer))
                        .expect("server_loop: database write");
                }
                db_write.commit().unwrap();
            });
        }

        let mut change_buffer = Vec::new();

        // Sending block class changes to players
        for (player, client, curr_radius) in self.actor_pc.iter().filter_map(|(player, actor)| {
            let client = self.client_pc.get(&player)?;
            let position = self.position_ac.get(&actor)?;
            let curr_view = self.chunk_view_pc.get(&player)?;
            let curr_radius = position.chunk.radius(curr_view.radius);

            Some((player, client, curr_radius))
        }) {
            let chunk_iter = self
                .class_bc
                .changed_chunks()
                .filter(|change| curr_radius.is_within(change.chunk));

            let chunk_amount = chunk_iter.clone().count();

            let mut change_encoder = ChunkChanges::encode_chunks(chunk_amount, &mut change_buffer);

            for chunk_change in chunk_iter {
                let mut block_encoder =
                    change_encoder.start_chunk(chunk_change.chunk, chunk_change.changes().len());

                for (block, block_class) in chunk_change.changes() {
                    block_encoder.add_change(*block, *block_class);
                }

                change_encoder = block_encoder.finish_chunk();
            }

            let changes = change_encoder.finish();

            let data = ClientAccept::ChunkChanges(changes);
            if client
                .tx
                .send(ClientEvent::SendDataReliable {
                    data: SendData::Owned(self.packer.pack_to_vec(&data)),
                })
                .is_err()
            {
                self.player_rq.enqueue(*player);
            }
        }

        self.class_bc.clear_changes();
    }
}
