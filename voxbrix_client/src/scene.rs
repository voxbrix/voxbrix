use crate::{
    window::WindowHandle,
    RenderHandle,
};
use anyhow::Result;
use game::{
    GameScene,
    GameSceneParameters,
};
use menu::MenuScene;

pub mod game;
pub mod menu;

pub enum SceneSwitch {
    Menu,
    Game { parameters: GameSceneParameters },
    Exit,
}

pub struct SceneManager {
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
}

impl SceneManager {
    pub async fn run(self) -> Result<()> {
        let mut next_loop = Some(SceneSwitch::Menu);

        loop {
            match next_loop.take().unwrap_or(SceneSwitch::Exit) {
                SceneSwitch::Menu => {
                    next_loop = Some(
                        MenuScene {
                            window_handle: self.window_handle,
                            render_handle: self.render_handle,
                        }
                        .run()
                        .await?,
                    );
                },
                SceneSwitch::Game { parameters } => {
                    next_loop = Some(
                        GameScene {
                            window_handle: self.window_handle,
                            render_handle: self.render_handle,
                            parameters,
                        }
                        .run()
                        .await?,
                    );
                },
                SceneSwitch::Exit => return Ok(()),
            }
        }
    }
}
