use crate::component::actor::position::GlobalPosition;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize)]
pub enum ServerAccept {
    PlayerPosition { position: GlobalPosition },
}
