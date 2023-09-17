use voxbrix_common::{
    entity::actor::Actor,
    sparse_vec::SparseVec,
};

pub struct ActorRegistry(SparseVec<()>);

impl ActorRegistry {
    pub fn new() -> Self {
        Self(SparseVec::new())
    }

    pub fn add(&mut self) -> Actor {
        Actor(self.0.push(()))
    }

    pub fn remove(&mut self, actor: &Actor) -> bool {
        self.0.remove(actor.0).is_some()
    }
}
