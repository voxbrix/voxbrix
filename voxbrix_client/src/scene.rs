use crate::{
    window::WindowHandle,
    RenderHandle,
};
use anyhow::Result;
use async_executor::LocalExecutor;
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

pub struct MainScene<'a> {
    pub rt: &'a LocalExecutor<'a>,
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
}

impl MainScene<'_> {
    pub async fn run(self) -> Result<()> {
        let mut next_loop = Some(SceneSwitch::Menu);

        loop {
            match next_loop.take().unwrap_or_else(|| SceneSwitch::Exit) {
                SceneSwitch::Menu => {
                    next_loop = Some(
                        MenuScene {
                            rt: self.rt,
                            window_handle: self.window_handle,
                            render_handle: self.render_handle,
                        }
                        .run()
                        .await?,
                    );
                },
                SceneSwitch::Game { parameters } => {
                    GameScene {
                        rt: self.rt,
                        window_handle: self.window_handle,
                        render_handle: self.render_handle,
                        parameters,
                    }
                    .run()
                    .await?;
                },
                SceneSwitch::Exit => return Ok(()),
            }
        }
    }
}
