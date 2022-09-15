use std::{
    any::{
        Any,
        TypeId,
    },
    collections::HashMap,
    sync::Arc,
};

pub struct StorageBuilder {
    data: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl StorageBuilder {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn add<T>(&mut self, new: T) -> &mut Self
    where
        T: Any + Send + Sync,
    {
        self.data.insert(new.type_id(), Arc::new(new));

        self
    }

    pub fn finish(self) -> Storage {
        Storage {
            data: Arc::new(self.data),
        }
    }
}

#[derive(Clone)]
pub struct Storage {
    data: Arc<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl Storage {
    pub fn get<T>(&self) -> Option<Arc<T>>
    where
        T: Any + Send + Sync,
    {
        self.data
            .get(&TypeId::of::<T>())
            .and_then(|r| r.downcast().ok())
    }
}
