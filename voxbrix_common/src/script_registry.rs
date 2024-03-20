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
use bincode::Options;
use nohash_hasher::IntMap;
use serde::Serialize;
use std::{
    fmt::Debug,
    mem,
    path::Path,
};
use tokio::task;
use wasmtime::{
    Engine,
    Instance,
    IntoFunc,
    Linker,
    Module,
    Store,
};

pub struct ScriptInstance<T> {
    pub store: Store<ScriptData<T>>,
    pub instance: Instance,
    /// Empty buffer, use for input serialization, etc.
    pub buffer: Vec<u8>,
}

struct ScriptCache<T> {
    store: Store<ScriptData<T>>,
    instance: Instance,
}

pub enum ScriptData<T> {
    Full(T),
    Empty,
}

impl<T> ScriptData<T> {
    pub fn into_ref(&self) -> &T {
        match self {
            ScriptData::Full(v) => &v,
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

    pub fn access_instance<U>(
        &mut self,
        script: &Script,
        data: U,
        mut access: impl FnMut(&mut ScriptInstance<U>),
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

        let instance = {
            let ScriptCache { store, instance } = self.cache.remove(script).unwrap();

            ScriptInstance {
                store,
                instance,
                buffer: mem::take(&mut self.buffer),
            }
        };

        let mut instance =
            unsafe { mem::transmute::<ScriptInstance<T>, ScriptInstance<U>>(instance) };

        *instance.store.data_mut() = ScriptData::Full(data);

        access(&mut instance);

        *instance.store.data_mut() = ScriptData::Empty;

        let ScriptInstance {
            store,
            instance,
            buffer,
        } = unsafe { mem::transmute::<ScriptInstance<U>, ScriptInstance<T>>(instance) };

        let cache = ScriptCache { store, instance };

        self.buffer = buffer;

        self.cache.insert(*script, cache);
    }

    pub fn run_script<U, I>(&mut self, script: &Script, data: U, input: I)
    where
        U: NonStatic<Static = T>,
        I: Serialize,
    {
        self.access_instance(script, data, |ins| {
            pack::packer()
                .serialize_into(&mut ins.buffer, &input)
                .expect("serialization should not fail");

            let input_len = ins.buffer.len() as u32;

            let get_write_buffer = ins
                .instance
                .get_typed_func::<u32, u32>(&mut ins.store, "get_buffer")
                .unwrap();

            let ptr = get_write_buffer
                .call(&mut ins.store, input_len)
                .expect("unable to get script input buffer");

            let memory = ins.instance.get_memory(&mut ins.store, "memory").unwrap();

            let start = ptr as usize;
            let end = start + input_len as usize;

            (&mut memory.data_mut(&mut ins.store)[start .. end])
                .copy_from_slice(ins.buffer.as_slice());

            let run = ins
                .instance
                .get_typed_func::<u32, ()>(&mut ins.store, "run")
                .unwrap();

            run.call(&mut ins.store, input_len)
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
