use voxbrix_common::{
    component::StaticEntityComponent,
    entity::actor_model::ActorModel,
};

pub mod builder;

pub type ActorModelComponent<T> = StaticEntityComponent<ActorModel, T>;
