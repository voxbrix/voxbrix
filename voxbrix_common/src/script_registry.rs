use crate::{
    entity::script::Script,
    pack,
    read_ron_file,
    system::list_loading::List,
    LabelMap,
};
use anyhow::{
    Context,
    Error,
};
use nohash_hasher::IntMap;
use serde::Serialize;
use std::{
    fmt::Debug,
    mem,
    path::Path,
};
use tokio::task;
use wasmtime::{
    AsContextMut,
    Engine,
    Instance,
    IntoFunc,
    Linker,
    Memory,
    Module,
    Store,
    TypedFunc,
};

pub struct ScriptCache<T> {
    pub store: Store<ScriptData<T>>,
    pub instance: Instance,
}

struct DynamicScriptData<T> {
    shared: T,
    buffer: Vec<u8>,
}

struct StaticScriptData {
    // Complete memory of the store.
    memory: Memory,
    // Common function that allows to allocate a buffer in the store of the given length and
    // returns a pointer to a memory inside the store.
    // Used to serialize input of the script.
    get_buffer_func: TypedFunc<u32, u32>,
}

pub struct ScriptData<T> {
    dynamic_data: Option<DynamicScriptData<T>>,
    static_data: Option<StaticScriptData>,
}

impl<T> ScriptData<T> {
    // Empty, unusable ScriptData for initializing the store.
    fn empty() -> Self {
        Self {
            dynamic_data: None,
            static_data: None,
        }
    }

    fn unset_dynamic(&mut self, common_buffer: &mut Vec<u8>) -> T {
        let data = self
            .dynamic_data
            .take()
            .expect("dynamic data is already unset");
        *common_buffer = data.buffer;
        data.shared
    }

    fn set_dynamic(&mut self, shared: T, common_buffer: &mut Vec<u8>) {
        self.dynamic_data = Some(DynamicScriptData {
            buffer: mem::take(common_buffer),
            shared,
        });
    }

    fn initialize(&mut self, memory: Memory, get_buffer_func: TypedFunc<u32, u32>) {
        self.static_data = Some(StaticScriptData {
            memory,
            get_buffer_func,
        });
    }

    pub fn shared(&self) -> &T {
        &self
            .dynamic_data
            .as_ref()
            .expect("dynamic data unset")
            .shared
    }

    pub fn shared_mut(&mut self) -> &mut T {
        &mut self
            .dynamic_data
            .as_mut()
            .expect("dynamic data unset")
            .shared
    }

    /// Complete memory of the store.
    pub fn memory(&self) -> Memory {
        self.static_data
            .as_ref()
            .expect("script data is not initialized")
            .memory
            .clone()
    }

    /// Common function that allows to allocate a buffer of the given length in
    /// the memory of the store.
    /// Returns a pointer to the allocated memory inside the store.
    pub fn get_buffer_func(&self) -> TypedFunc<u32, u32> {
        self.static_data
            .as_ref()
            .expect("script data is not initialized")
            .get_buffer_func
            .clone()
    }

    /// Common buffer that can be used for e.g. serializing into it before copying to the store
    /// memory.
    pub fn buffer(&mut self) -> &mut Vec<u8> {
        let buf = &mut self
            .dynamic_data
            .as_mut()
            .expect("dynamic data unset")
            .buffer;

        buf.clear();

        buf
    }
}

/// Calls `get_buffer(len: 32) -> *const u8` in the script and
/// writes at the pointer the whatever you put in the buffer.
pub fn write_script_buffer<T>(
    mut store: impl AsContextMut<Data = ScriptData<T>>,
    value: impl Serialize,
) {
    let mut store_data = store.as_context_mut();
    let store_data = store_data.data_mut();
    let mut buffer = mem::take(store_data.buffer());

    let get_buffer_func = store_data.get_buffer_func();
    let memory = store_data.memory();

    buffer.clear();
    pack::encode_into(&value, &mut buffer);

    let input_len = buffer.len() as u32;

    let ptr = get_buffer_func
        .call(&mut store, input_len)
        .expect("unable to get script input buffer");

    let start = ptr as usize;
    let end = start + input_len as usize;

    (&mut memory.data_mut(&mut store)[start .. end]).copy_from_slice(buffer.as_slice());

    *store.as_context_mut().data_mut().buffer() = buffer;
}

pub struct ScriptRegistry<T> {
    engine: Engine,
    label_map: LabelMap<Script>,
    modules: Vec<Module>,
    linker: Linker<ScriptData<T>>,
    cache: IntMap<Script, ScriptCache<T>>,
    buffer: Vec<u8>,
}

impl<T> ScriptRegistry<T> {
    pub fn get_script_by_label(&self, label: &str) -> Option<Script> {
        self.label_map.get(label)
    }

    pub fn script_label_map(&self) -> &LabelMap<Script> {
        &self.label_map
    }

    pub fn func_wrap<Params, Args>(
        &mut self,
        module: &str,
        name: &str,
        func: impl IntoFunc<ScriptData<T>, Params, Args>,
    ) {
        self.linker.func_wrap(module, name, func).unwrap();
    }

    pub fn access_script(
        &mut self,
        script: &Script,
        shared: T,
        mut access: impl FnMut(&mut ScriptCache<T>),
    ) -> T {
        if !self.cache.contains_key(script) {
            let mut store = Store::new(&self.engine, ScriptData::empty());

            let module = self
                .modules
                .get(script.0 as usize)
                .expect("script not found");

            let instance = self
                .linker
                .instantiate(&mut store, module)
                .expect("instantiation should not fail");

            let get_buffer_func = instance
                .get_typed_func::<u32, u32>(&mut store, "get_buffer")
                .unwrap();

            let memory = instance.get_memory(&mut store, "memory").unwrap();

            store.data_mut().initialize(memory, get_buffer_func);

            self.cache.insert(*script, ScriptCache { store, instance });
        }

        self.buffer.clear();

        let mut cache = self.cache.remove(script).unwrap();

        cache.store.data_mut().set_dynamic(shared, &mut self.buffer);

        access(&mut cache);

        let shared = cache.store.data_mut().unset_dynamic(&mut self.buffer);

        self.cache.insert(*script, cache);

        shared
    }

    pub fn run_script<I>(&mut self, script: &Script, shared: T, input: I) -> T
    where
        I: Serialize,
    {
        self.access_script(script, shared, |bundle| {
            write_script_buffer(&mut bundle.store, &input);

            let run = bundle
                .instance
                .get_typed_func::<(), ()>(&mut bundle.store, "run")
                .unwrap();

            run.call(&mut bundle.store, ())
                .expect("unable to run script");
        })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub async fn load(
        engine: Engine,
        list_path: impl 'static + AsRef<Path> + Debug + Send + Clone,
        dir_path: impl 'static + AsRef<Path> + Debug + Send,
    ) -> Result<Self, Error> {
        let list = {
            let list_path = list_path.clone();

            task::spawn_blocking(move || read_ron_file::<List>(list_path))
                .await
                .unwrap()
        }
        .with_context(|| format!("unable to load list \"{:?}\"", list_path))?;

        let engine_clone = engine.clone();

        let label_map = list.clone().into_label_map();

        let modules = task::spawn_blocking(move || {
            list.list
                .into_iter()
                .map(|file_name| {
                    let file_path = dir_path.as_ref().join(file_name).with_extension("wasm");

                    Module::from_file(&engine_clone, &file_path)
                        .map_err(|err| Error::from(err))
                        .with_context(|| {
                            format!("unable to load script module from \"{:?}\"", file_path)
                        })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .await
        .unwrap()?;

        let linker = Linker::new(&engine);

        Ok(Self {
            engine,
            label_map,
            modules,
            linker,
            cache: IntMap::default(),
            buffer: Vec::new(),
        })
    }
}
