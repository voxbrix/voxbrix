use crate::window::Window;
use anyhow::Result;
use game::{
    GameScene,
    GameSceneParameters,
};
use menu::{
    MenuScene,
    MenuSceneParameters,
};

pub mod game;
pub mod menu;

pub enum SceneSwitch {
    Menu { parameters: MenuSceneParameters },
    Game { parameters: GameSceneParameters },
    Exit,
}

pub struct SceneManager {
    pub window: Window,
}

impl SceneManager {
    pub async fn run(self) -> Result<()> {
        let Self { window } = self;

        let mut next_loop = Some(SceneSwitch::Menu {
            parameters: MenuSceneParameters { window },
        });

        loop {
            match next_loop.take().unwrap_or(SceneSwitch::Exit) {
                SceneSwitch::Menu { parameters } => {
                    next_loop = Some(MenuScene { parameters }.run().await?);
                },
                SceneSwitch::Game { parameters } => {
                    next_loop = Some(GameScene { parameters }.run().await?);
                },
                SceneSwitch::Exit => return Ok(()),
            }
        }
    }
}
