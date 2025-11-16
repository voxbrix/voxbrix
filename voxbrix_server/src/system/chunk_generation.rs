use crate::{
    assets::{
        CHUNK_GENERATION_SCRIPT_DIR,
        CHUNK_GENERATION_SCRIPT_LIST,
        DIMENSION_KIND_GENERATION_MAP,
    },
    resource::chunk_generation_request::ChunkGenerationRequest,
    storage::{
        IntoData,
        IntoDataSized,
    },
    system::map_loading::Map,
    BLOCK_CLASS_TABLE,
    BLOCK_ENVIRONMENT_TABLE,
    BLOCK_METADATA_TABLE,
};
use anyhow::{
    Context,
    Error,
};
use flume::Sender;
use redb::Database;
use std::{
    mem,
    path::PathBuf,
    sync::Arc,
    thread,
};
use tokio::task;
use voxbrix_common::{
    component::block::{
        metadata::BlockMetadata,
        BlocksVec,
        BlocksVecBuilder,
    },
    entity::{
        block::BLOCKS_IN_CHUNK_EDGE,
        block_class::BlockClass,
        block_environment::BlockEnvironment,
        chunk::{
            Chunk,
            Dimension,
            DimensionKind,
        },
        script::Script,
    },
    pack::Packer,
    read_data_file,
    AsFromUsize,
    LabelLibrary,
    LabelMap,
};
use voxbrix_world::{
    System,
    SystemData,
};
use wasmtime::{
    Caller,
    Config,
    Engine,
    Linker,
    Module,
    Store,
};

#[derive(SystemData)]
pub struct ChunkGenerationSystemData<'a> {
    database: &'a Arc<Database>,
    label_library: &'a LabelLibrary,
}

pub struct ChunkGenerationSystem;

impl System for ChunkGenerationSystem {
    type Data<'a> = ChunkGenerationSystemData<'a>;
}

struct GenerationData {
    block_class_label_map: LabelMap<BlockClass>,
    block_environment_label_map: LabelMap<BlockEnvironment>,
    block_classes: BlocksVecBuilder<BlockClass>,
    block_environment: BlocksVecBuilder<BlockEnvironment>,
    block_metadata: BlocksVecBuilder<BlockMetadata>,
}

impl ChunkGenerationSystemData<'_> {
    #[must_use]
    pub async fn spawn(
        self,
        send_chunk_data: impl Fn(
                Chunk,
                BlocksVec<BlockClass>,
                BlocksVec<BlockEnvironment>,
                BlocksVec<BlockMetadata>,
                &mut Packer,
            ) + Send
            + 'static,
    ) -> Sender<ChunkGenerationRequest> {
        let database = self.database.clone();

        let block_class_label_map = self
            .label_library
            .get_label_map_for::<BlockClass>()
            .expect("block class label map not found");
        let block_environment_label_map = self
            .label_library
            .get_label_map_for::<BlockEnvironment>()
            .expect("block environment label map not found");

        let dimension_kind_label_map = self
            .label_library
            .get_label_map_for::<DimensionKind>()
            .expect("dimension kind label map not found");

        let (new_chunks_tx, new_chunks_rx) = flume::unbounded();

        let list = {
            task::spawn_blocking(move || {
                read_data_file::<Vec<String>>(CHUNK_GENERATION_SCRIPT_LIST)
            })
            .await
            .unwrap()
        }
        .with_context(|| format!("unable to load list \"{:?}\"", CHUNK_GENERATION_SCRIPT_LIST))
        .unwrap();

        let script_labels = LabelMap::<Script>::from_list(&list);

        let dimension_kind_script_map = Map::<String>::load(DIMENSION_KIND_GENERATION_MAP)
            .await
            .expect("unable to load dimension kind chunk generation script map");

        let dimension_scripts = dimension_kind_label_map
            .iter()
            .map(|(_, dimension_label)| {
                let script_label = dimension_kind_script_map
                    .get(dimension_label)
                    .map(|s| s.to_owned())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "no script for dimension kind \"{}\" defined",
                            dimension_label,
                        )
                    })?;

                script_labels
                    .get(&script_label)
                    .ok_or_else(|| anyhow::anyhow!("no script \"{}\" defined", script_label))?;

                Ok(script_label.clone())
            })
            .collect::<Result<Vec<_>, Error>>()
            .expect("unable to define scripts for dimension generation");

        thread::spawn(move || {
            let mut engine_config = Config::new();

            engine_config
                .wasm_multi_value(false)
                .wasm_multi_memory(false);

            let engine = Engine::new(&engine_config).expect("unable to initialize wasm engine");

            let mut linker = Linker::new(&engine);

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
                    move |mut caller: Caller<'_, GenerationData>, ptr: u32, len: u32| -> u32 {
                        let ptr = ptr as usize;
                        let len = len as usize;
                        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
                        let label =
                            std::str::from_utf8(&memory.data(&caller)[ptr .. ptr + len]).unwrap();
                        caller
                            .data()
                            .block_class_label_map
                            .get(label)
                            .expect("block class label not found")
                            .0 as u32
                    },
                )
                .unwrap();

            linker
                .func_wrap(
                    "env",
                    "get_block_environment",
                    move |mut caller: Caller<'_, GenerationData>, ptr: u32, len: u32| -> u32 {
                        let ptr = ptr as usize;
                        let len = len as usize;
                        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
                        let label =
                            std::str::from_utf8(&memory.data(&caller)[ptr .. ptr + len]).unwrap();
                        caller
                            .data()
                            .block_environment_label_map
                            .get(label)
                            .expect("block class label not found")
                            .0 as u32
                    },
                )
                .unwrap();

            linker
                .func_wrap(
                    "env",
                    "push_block",
                    |mut caller: Caller<'_, GenerationData>, block_data: u32| {
                        let bclm_len = caller.data().block_class_label_map.len();
                        let bmedlm_len = caller.data().block_environment_label_map.len();

                        let block_class = ((block_data >> 16) & 0xFF) as u16;
                        let block_environment = ((block_data >> 8) & 0xF) as u8;
                        let block_metadata = (block_data & 0xF) as u8;

                        assert!(
                            (block_class as usize) < bclm_len,
                            "incorrect block class generated"
                        );
                        assert!(
                            (block_environment as usize) < bmedlm_len,
                            "incorrect block environment generated"
                        );

                        caller
                            .data_mut()
                            .block_classes
                            .push(BlockClass(block_class));

                        caller
                            .data_mut()
                            .block_environment
                            .push(BlockEnvironment(block_environment));

                        caller
                            .data_mut()
                            .block_metadata
                            .push(BlockMetadata(block_metadata));
                    },
                )
                .unwrap();

            let mut path_buf: PathBuf = CHUNK_GENERATION_SCRIPT_DIR
                .parse()
                .expect("unable to parse chunk generation script dir path");

            let mut modules = Vec::with_capacity(dimension_scripts.len());

            for label in dimension_scripts.iter() {
                path_buf.push(label);
                path_buf.set_extension("wasm");

                let module = Module::from_file(&engine, &path_buf)
                    .expect("unable to load chunk generation script module");

                modules.push(module);

                path_buf.pop();
            }

            let mut packer = Packer::new();

            let seed = 0;

            while let Ok(request) = new_chunks_rx.recv() {
                let ChunkGenerationRequest { chunk } = request;
                let Chunk {
                    position,
                    dimension: Dimension { kind, phase },
                } = chunk;

                let mut store = Store::new(
                    &engine,
                    GenerationData {
                        block_class_label_map: block_class_label_map.clone(),
                        block_environment_label_map: block_environment_label_map.clone(),
                        block_classes: BlocksVecBuilder::new(),
                        block_environment: BlocksVecBuilder::new(),
                        block_metadata: BlocksVecBuilder::new(),
                    },
                );

                let module = modules
                    .get(kind.as_usize())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "unable to find generation script for dimension kind \"{}\"",
                            dimension_kind_label_map.get_label(&kind).unwrap(),
                        )
                    })
                    .unwrap();

                let instance = linker.instantiate(&mut store, &module).unwrap();

                let generate_fn = instance
                    .get_typed_func::<(u64, u64, i32, i32, i32), ()>(&mut store, "generate_chunk")
                    .unwrap();

                generate_fn
                    .call(
                        &mut store,
                        (seed, phase, position[0], position[1], position[2]),
                    )
                    .expect("generate_fn call error");

                let block_classes =
                    mem::replace(&mut store.data_mut().block_classes, BlocksVecBuilder::new())
                        .build();

                let block_environment = mem::replace(
                    &mut store.data_mut().block_environment,
                    BlocksVecBuilder::new(),
                )
                .build();

                let block_metadata = mem::replace(
                    &mut store.data_mut().block_metadata,
                    BlocksVecBuilder::new(),
                )
                .build();

                let db_write = database.begin_write().unwrap();
                {
                    let mut table = db_write.open_table(BLOCK_CLASS_TABLE).unwrap();
                    table
                        .insert(
                            chunk.into_data_sized(),
                            block_classes.into_data(&mut packer),
                        )
                        .expect("server_loop: database write");

                    let mut table = db_write.open_table(BLOCK_ENVIRONMENT_TABLE).unwrap();
                    table
                        .insert(
                            chunk.into_data_sized(),
                            block_environment.into_data(&mut packer),
                        )
                        .expect("server_loop: database write");

                    let mut table = db_write.open_table(BLOCK_METADATA_TABLE).unwrap();
                    table
                        .insert(
                            chunk.into_data_sized(),
                            block_metadata.into_data(&mut packer),
                        )
                        .expect("server_loop: database write");
                }
                db_write.commit().unwrap();

                send_chunk_data(
                    chunk,
                    block_classes,
                    block_environment,
                    block_metadata,
                    &mut packer,
                );
            }
        });

        new_chunks_tx
    }
}
