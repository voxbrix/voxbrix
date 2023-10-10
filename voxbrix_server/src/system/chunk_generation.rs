use crate::{
    assets::CHUNK_GENERATION_SCRIPT,
    storage::{
        IntoData,
        IntoDataSized,
    },
    Shared,
    BLOCK_CLASS_TABLE,
};
use flume::Sender;
use std::{
    mem,
    thread,
};
use voxbrix_common::{
    component::block::BlocksVec,
    entity::{
        block::{
            BLOCKS_IN_CHUNK_EDGE,
            BLOCKS_IN_CHUNK_USIZE,
        },
        block_class::BlockClass,
        chunk::{
            Chunk,
            Dimension,
        },
    },
    pack::Packer,
    LabelMap,
};
use wasmtime::{
    Caller,
    Config,
    Engine,
    Linker,
    Module,
    Store,
};

pub struct ChunkGenerationSystem {
    new_chunks_tx: Sender<Chunk>,
}

struct GenerationData {
    block_class_label_map: LabelMap<BlockClass>,
    block_classes: Vec<BlockClass>,
}

impl ChunkGenerationSystem {
    pub fn new(
        shared: &'static Shared,
        block_class_label_map: LabelMap<BlockClass>,
        send_chunk_data: impl Fn(Chunk, BlocksVec<BlockClass>, &mut Packer) + Send + 'static,
    ) -> Self {
        let (new_chunks_tx, new_chunks_rx) = flume::unbounded();

        thread::spawn(move || {
            let mut engine_config = Config::new();

            engine_config
                .wasm_threads(false)
                .wasm_reference_types(false)
                .wasm_multi_value(false)
                .wasm_multi_memory(false);

            let engine = Engine::new(&engine_config).expect("unable to initialize wasm engine");

            let module = Module::from_file(&engine, CHUNK_GENERATION_SCRIPT).unwrap();
            let mut linker = Linker::new(&engine);
            let mut store = Store::new(
                &engine,
                GenerationData {
                    block_class_label_map,
                    block_classes: Vec::with_capacity(BLOCKS_IN_CHUNK_USIZE),
                },
            );

            linker
                .func_wrap(
                    "env",
                    "get_blocks_in_chunk_edge",
                    move |_caller: Caller<'_, GenerationData>| -> u32 {
                        BLOCKS_IN_CHUNK_EDGE as u32
                    },
                )
                .unwrap();

            linker
                .func_wrap(
                    "env",
                    "get_block_class",
                    move |mut caller: Caller<'_, GenerationData>, ptr: u32, len: u32| -> u64 {
                        let ptr = ptr as usize;
                        let len = len as usize;
                        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
                        let label =
                            std::str::from_utf8(&memory.data(&caller)[ptr .. ptr + len]).unwrap();
                        caller.data().block_class_label_map.get(label).unwrap().0
                    },
                )
                .unwrap();

            linker
                .func_wrap(
                    "env",
                    "push_block",
                    |mut caller: Caller<'_, GenerationData>, block_class: u64| {
                        caller
                            .data_mut()
                            .block_classes
                            .push(BlockClass(block_class));
                    },
                )
                .unwrap();

            let instance = linker.instantiate(&mut store, &module).unwrap();

            let generate_fn = instance
                .get_typed_func::<(u64, i32, i32, i32), ()>(&mut store, "generate_chunk")
                .unwrap();

            let mut packer = Packer::new();

            let seed = 0;

            while let Ok(chunk) = new_chunks_rx.recv() {
                let Chunk {
                    position,
                    dimension: Dimension { index: _ },
                } = chunk;

                generate_fn.call(&mut store, (seed, position.x, position.y, position.z));

                let block_classes = BlocksVec::new(mem::replace(
                    &mut store.data_mut().block_classes,
                    Vec::with_capacity(BLOCKS_IN_CHUNK_USIZE),
                ));

                let db_write = shared.database.begin_write().unwrap();
                {
                    let mut table = db_write.open_table(BLOCK_CLASS_TABLE).unwrap();
                    table
                        .insert(
                            chunk.into_data_sized(),
                            block_classes.into_data(&mut packer),
                        )
                        .expect("server_loop: database write");
                }
                db_write.commit().unwrap();

                send_chunk_data(chunk, block_classes, &mut packer);
            }
        });

        Self { new_chunks_tx }
    }

    pub fn generate_chunk(&self, chunk: Chunk) {
        let _ = self.new_chunks_tx.send(chunk);
    }
}
