use crate::{
    system::render::output_thread::OutputThread,
    window::WindowHandle,
    RenderHandle,
};
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
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
    pub interface_state: egui_winit::State,
    pub output_thread: OutputThread,
}

impl SceneManager {
    pub async fn run(self) -> Result<()> {
        let Self {
            window_handle,
            render_handle,
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
                    next_loop = Some(
                        MenuScene {
                            window_handle,
                            render_handle,
                            parameters,
                        }
                        .run()
                        .await?,
                    );
                },
                SceneSwitch::Game { parameters } => {
                    next_loop = Some(
                        GameScene {
                            window_handle,
                            render_handle,
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
