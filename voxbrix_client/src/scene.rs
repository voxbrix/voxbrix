use crate::system::render::output_thread::OutputThread;
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
    pub interface_state: egui_winit::State,
    pub output_thread: OutputThread,
}

impl SceneManager {
    pub async fn run(self) -> Result<()> {
        let Self {
            interface_state,
            output_thread,
        } = self;

        let mut next_loop = Some(SceneSwitch::Menu {
            parameters: MenuSceneParameters {
                interface_state,
                output_thread,
            },
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
