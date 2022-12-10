use crate::{
    component::actor::position::GlobalPosition,
    pack::PackDefault,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize)]
pub enum ServerAccept {
    PlayerPosition { position: GlobalPosition },
}

impl PackDefault for ServerAccept {}
