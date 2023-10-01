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
        Actor::from_usize(self.0.push(()))
    }

    pub fn remove(&mut self, actor: &Actor) -> bool {
        self.0.remove(actor.into_usize()).is_some()
    }
}
