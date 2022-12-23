use crate::{
    component::player::PlayerComponent,
    server::Client,
};

pub type ClientPlayerComponent = PlayerComponent<Client>;
