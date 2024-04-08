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
use bincode::Encode;
use nohash_hasher::IntMap;
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

pub struct ScriptDataFull<T> {
    pub data: T,
    /// Complete memory of the store.
    pub memory: Memory,
    buffer: Vec<u8>,
    get_buffer_func: TypedFunc<u32, u32>,
}

/// Calls `get_buffer(len: 32) -> *const u8` in the script and
/// writes at the pointer the whatever you put in the buffer.
/// Returns written length.
pub fn write_script_buffer<T>(
    mut store: impl AsContextMut<Data = ScriptData<T>>,
    value: impl Encode,
) -> u32 {
    let mut store_data = store.as_context_mut();
    let store_data = store_data.data_mut().as_full_mut();
    let mut buffer = mem::take(&mut store_data.buffer);

    let get_buffer_func = store_data.get_buffer_func.clone();
    let memory = store_data.memory.clone();

    buffer.clear();
    pack::encode_into(&value, &mut buffer);

    let input_len = buffer.len() as u32;

    let ptr = get_buffer_func
        .call(&mut store, input_len)
        .expect("unable to get script input buffer");

    let start = ptr as usize;
    let end = start + input_len as usize;

    (&mut memory.data_mut(&mut store)[start .. end]).copy_from_slice(buffer.as_slice());

    store.as_context_mut().data_mut().as_full_mut().buffer = buffer;

    input_len
}

pub enum ScriptData<T> {
    Full(ScriptDataFull<T>),
    Empty,
}

impl<T> ScriptData<T> {
    pub fn into_full(self) -> ScriptDataFull<T> {
        match self {
            ScriptData::Full(v) => v,
            ScriptData::Empty => panic!("script data is empty"),
        }
    }

    pub fn as_full_mut(&mut self) -> &mut ScriptDataFull<T> {
        match self {
            ScriptData::Full(v) => v,
            ScriptData::Empty => panic!("script data is empty"),
        }
    }

    pub fn as_full(&self) -> &ScriptDataFull<T> {
        match self {
            ScriptData::Full(v) => v,
            ScriptData::Empty => panic!("script data is empty"),
        }
    }
}

pub unsafe trait NonStatic {
    type Static;
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

    /// Safety: For non-static T, the references inside
    /// will only be valid within the scope of the `func`.
    /// Make sure you provide anonymous lifetime as
    /// lifetime of `T` while wrapping host functions for non-static `T`.
    /// For example:
    /// `ScriptData<ScriptSharedData<'_>>`.
    pub unsafe fn func_wrap<Params, Args>(
        &mut self,
        module: &str,
        name: &str,
        func: impl IntoFunc<ScriptData<T>, Params, Args>,
    ) {
        self.linker.func_wrap(module, name, func).unwrap();
    }

    pub fn access_script<U>(
        &mut self,
        script: &Script,
        data: U,
        mut access: impl FnMut(&mut ScriptCache<U>),
    ) where
        U: NonStatic<Static = T>,
    {
        if !self.cache.contains_key(script) {
            let mut store = Store::new(&self.engine, ScriptData::Empty);
            let module = self
                .modules
                .get(script.0 as usize)
                .expect("script not found");
            let instance = self
                .linker
                .instantiate(&mut store, module)
                .expect("instantiation should not fail");

            self.cache.insert(*script, ScriptCache { store, instance });
        }

        self.buffer.clear();

        let cache = self.cache.remove(script).unwrap();

        let mut cache = unsafe { mem::transmute::<ScriptCache<T>, ScriptCache<U>>(cache) };

        let get_buffer_func = cache
            .instance
            .get_typed_func::<u32, u32>(&mut cache.store, "get_buffer")
            .unwrap();

        let memory = cache
            .instance
            .get_memory(&mut cache.store, "memory")
            .unwrap();

        *cache.store.data_mut() = ScriptData::Full(ScriptDataFull {
            data,
            get_buffer_func,
            memory,
            buffer: mem::take(&mut self.buffer),
        });

        access(&mut cache);

        let full = mem::replace(cache.store.data_mut(), ScriptData::Empty);

        self.buffer = full.into_full().buffer;

        let cache = unsafe { mem::transmute::<ScriptCache<U>, ScriptCache<T>>(cache) };

        self.cache.insert(*script, cache);
    }

    pub fn run_script<U, I>(&mut self, script: &Script, data: U, input: I)
    where
        U: NonStatic<Static = T>,
        I: Encode,
    {
        self.access_script(script, data, |bundle| {
            let input_len = write_script_buffer(&mut bundle.store, &input);

            let run = bundle
                .instance
                .get_typed_func::<u32, ()>(&mut bundle.store, "run")
                .unwrap();

            run.call(&mut bundle.store, input_len)
                .expect("unable to run script");
        });
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

        let label_map = list
            .clone()
            .into_label_map(|i| Script(i.try_into().unwrap()));

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
