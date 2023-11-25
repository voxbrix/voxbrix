use crate::{
    scene::game::{
        data::GameSharedData,
        Transition,
    },
    window::InputEvent,
};
use voxbrix_common::{
    entity::block::Block,
    messages::server::ServerAccept,
};
use winit::event::{
    DeviceEvent,
    ElementState,
    MouseButton,
    WindowEvent,
};

pub struct LocalInput<'a> {
    pub shared_data: &'a mut GameSharedData,
    pub event: InputEvent,
}

impl LocalInput<'_> {
    pub fn run(self) -> Transition {
        let LocalInput {
            shared_data: sd,
            event,
        } = self;

        match event {
            InputEvent::DeviceEvent {
                device_id: _,
                event,
            } => {
                if !sd.inventory_open {
                    match event {
                        DeviceEvent::MouseMotion {
                            delta: (horizontal, vertical),
                        } => {
                            sd.direct_control_system
                                .process_mouse(horizontal as f32, vertical as f32);
                        },
                        _ => {},
                    }
                }
            },
            InputEvent::WindowEvent { event } => {
                if sd.inventory_open {
                    sd.interface_system.window_event(&event);
                }
                match event {
                    WindowEvent::Resized(size) => {
                        sd.render_system.resize(size);
                    },
                    WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                        return Transition::Exit;
                    },
                    WindowEvent::KeyboardInput {
                        device_id: _,
                        input,
                        is_synthetic: _,
                    } => {
                        if let Some(button) = input.virtual_keycode {
                            if matches!(input.state, winit::event::ElementState::Pressed) {
                                match button {
                                    winit::event::VirtualKeyCode::Escape => {
                                        return Transition::Menu;
                                    },
                                    winit::event::VirtualKeyCode::I => {
                                        sd.inventory_open = !sd.inventory_open;
                                    },
                                    _ => {},
                                }
                            }
                        }
                        sd.direct_control_system.process_keyboard(&input);
                    },
                    WindowEvent::MouseInput { state, button, .. } => {
                        if state == ElementState::Pressed {
                            match button {
                                MouseButton::Left => {
                                    if let Some((chunk, block, _side)) =
                                        sd.player_position_system.get_target_block(
                                            &sd.position_ac,
                                            &sd.orientation_ac,
                                            |chunk, block| {
                                                sd.class_bc
                                                    .get_chunk(&chunk)
                                                    .map(|blocks| {
                                                        let class = blocks.get(block);
                                                        sd.collision_bcc.get(class).is_some()
                                                    })
                                                    .unwrap_or(false)
                                            },
                                        )
                                    {
                                        let _ = sd.reliable_tx.send(sd.packer.pack_to_vec(
                                            &ServerAccept::AlterBlock {
                                                chunk,
                                                block,
                                                block_class:
                                                    sd.block_class_label_map.get("air").unwrap(),
                                            },
                                        ));
                                    }
                                },
                                MouseButton::Right => {
                                    if let Some((chunk, block, side)) =
                                        sd.player_position_system.get_target_block(
                                            &sd.position_ac,
                                            &sd.orientation_ac,
                                            |chunk, block| {
                                                sd.class_bc
                                                    .get_chunk(&chunk)
                                                    .map(|blocks| {
                                                        let class = blocks.get(block);
                                                        sd.collision_bcc.get(class).is_some()
                                                    })
                                                    .unwrap_or(false)
                                            },
                                        )
                                    {
                                        let axis = side / 2;
                                        let direction = match side % 2 {
                                            0 => -1,
                                            1 => 1,
                                            _ => panic!("incorrect side index"),
                                        };
                                        let mut block = block.to_coords().map(|u| u as i32);
                                        block[axis] += direction;
                                        if let Some((chunk, block)) =
                                            Block::from_chunk_offset(chunk, block)
                                        {
                                            let _ = sd.reliable_tx.send(
                                                sd.packer.pack_to_vec(&ServerAccept::AlterBlock {
                                                    chunk,
                                                    block,
                                                    block_class: sd
                                                        .block_class_label_map
                                                        .get("grass")
                                                        .unwrap(),
                                                }),
                                            );
                                        }
                                    }
                                },
                                _ => {},
                            }
                        }
                    },
                    _ => {},
                }
            },
        }

        Transition::None
    }
}
