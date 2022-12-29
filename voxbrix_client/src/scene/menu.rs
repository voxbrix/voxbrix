use crate::{
    scene::{
        game::GameSceneParameters,
        SceneSwitch,
    },
    window::WindowHandle,
    RenderHandle,
};
use anyhow::Result;
use async_executor::LocalExecutor;

pub struct MenuScene<'a> {
    pub rt: &'a LocalExecutor<'a>,
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
}

impl MenuScene<'_> {
    pub async fn run(self) -> Result<SceneSwitch> {
        Ok(SceneSwitch::Game {
            parameters: GameSceneParameters {
                socket: ([127, 0, 0, 1], 0).into(),
                server: ([127, 0, 0, 1], 12000).into(),
                username: "username".to_owned(),
                password: "password".as_bytes().to_owned(),
            },
        })
    }
}
