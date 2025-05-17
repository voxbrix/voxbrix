use std::{
    any::{
        Any,
        TypeId,
    },
    cell::UnsafeCell,
    collections::HashMap,
    marker::PhantomData,
};
#[cfg(feature = "derive")]
pub use voxbrix_ecs_derive::SystemArgs;

struct Compiled<T> {
    args: Vec<Request<usize>>,
    args_type: PhantomData<T>,
}

pub struct World {
    storage: Vec<UnsafeCell<Option<Box<dyn Any + Send + Sync>>>>,
    borrowed: Vec<TypeId>,
    borrowed_mut: Vec<TypeId>,
    type_map: HashMap<TypeId, usize>,
}

impl World {
    pub fn new() -> Self {
        Self {
            storage: Vec::new(),
            borrowed: Vec::new(),
            borrowed_mut: Vec::new(),
            type_map: HashMap::new(),
        }
    }

    pub fn insert<T>(&mut self, resource: T)
    where
        T: Send + Sync + 'static,
    {
        let idx = self.storage.len();

        let type_id = TypeId::of::<T>();

        if self.type_map.insert(type_id, idx).is_some() {
            panic!("resouce \"{:?}\" is already defined", type_id);
        }

        self.storage.push(UnsafeCell::new(Some(Box::new(resource))));
    }

    fn compile<'a, S>(&mut self) -> Compiled<S>
    where
        S: System + Send + Sync,
    {
        self.borrowed.clear();
        self.borrowed_mut.clear();

        let args = S::Args::required_resources()
            .map(|req| {
                match req {
                    Request::Read(type_id) => {
                        if self.borrowed_mut.contains(&type_id) {
                            panic!("resource \"{:?}\" already mutably borrowed", type_id);
                        }

                        let idx = self.type_map.get(&type_id).unwrap_or_else(|| {
                            panic!("resource \"{:?}\" is undefined", type_id);
                        });

                        self.borrowed.push(type_id);

                        Request::Read(*idx)
                    },
                    Request::Write(type_id) => {
                        if self.borrowed.contains(&type_id) {
                            panic!("resource \"{:?}\" already borrowed", type_id);
                        }

                        if self.borrowed_mut.contains(&type_id) {
                            panic!("resource \"{:?}\" already mutably borrowed", type_id);
                        }

                        let idx = self.type_map.get(&type_id).unwrap_or_else(|| {
                            panic!("resource \"{:?}\" is undefined", type_id);
                        });

                        self.borrowed_mut.push(type_id);

                        Request::Write(*idx)
                    },
                }
            })
            .collect();

        Compiled {
            args,
            args_type: Default::default(),
        }
    }

    pub fn get_args<'a, S>(&'a mut self) -> S::Args<'a>
    where
        S: System + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<Compiled<S>>();

        let idx = match self.type_map.get(&type_id).copied() {
            Some(idx) => idx,
            None => {
                let compiled = self.compile::<S>();

                self.insert(compiled);

                *self.type_map.get(&type_id).unwrap()
            },
        };

        let ptr = self.storage.get(idx).unwrap().get();

        let bx = unsafe { &*ptr }.as_ref().unwrap_or_else(|| {
            panic!("resource \"{:?}\" is taken", type_id);
        });

        let cmpd = bx.downcast_ref::<Compiled<S>>().unwrap();

        let access_iter = cmpd.args.iter().map(|req| {
            match req {
                Request::Read(idx) => {
                    let ptr = self.storage.get(*idx).unwrap().get();

                    let bx = unsafe { &*ptr }
                        .as_ref()
                        .unwrap_or_else(|| {
                            panic!("resource \"{:?}\" is taken", type_id);
                        })
                        .as_ref();

                    Access::Read(bx)
                },
                Request::Write(idx) => {
                    let ptr = self.storage.get(*idx).unwrap().get();

                    let bx = unsafe { &mut *ptr }
                        .as_mut()
                        .unwrap_or_else(|| {
                            panic!("resource \"{:?}\" is taken", type_id);
                        })
                        .as_mut();

                    Access::Write(bx)
                },
            }
        });

        S::Args::from_resources(access_iter)
    }
}

pub enum Request<T> {
    Read(T),
    Write(T),
}

pub enum Access<'a, T: ?Sized> {
    Read(&'a T),
    Write(&'a mut T),
}

impl<'a> Access<'a, dyn Any + Send + Sync> {
    pub fn downcast_ref<T>(self) -> &'a T
    where
        T: 'static,
    {
        let Access::Read(r) = self else {
            panic!("expected Read access but Write access provided");
        };

        r.downcast_ref::<T>().expect("incorrect type in access")
    }

    pub fn downcast_mut<T>(self) -> &'a mut T
    where
        T: 'static,
    {
        let Access::Write(r) = self else {
            panic!("expected Write access but Read access provided");
        };

        r.downcast_mut::<T>().expect("incorrect type in access")
    }
}

pub trait System {
    type Args<'a>: SystemArgs<'a>;
}

pub trait SystemArgs<'a> {
    fn required_resources() -> impl Iterator<Item = Request<TypeId>>;

    fn from_resources(resources: impl Iterator<Item = Access<'a, dyn Any + Send + Sync>>) -> Self;
}
