use crate::{
    scene::game::{
        data::GameSharedData,
        Transition,
    },
    window::InputEvent,
};
use voxbrix_common::entity::block::Block;
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
                    sd.interface_system
                        .window_event(sd.render_system.output_thread().window(), &event);
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
                        event,
                        is_synthetic: _,
                    } => {
                        if let winit::keyboard::PhysicalKey::Code(button) = event.physical_key {
                            if matches!(event.state, winit::event::ElementState::Pressed) {
                                match button {
                                    winit::keyboard::KeyCode::Escape => {
                                        return Transition::Menu;
                                    },
                                    winit::keyboard::KeyCode::KeyI => {
                                        sd.inventory_open = !sd.inventory_open;
                                    },
                                    _ => {},
                                }
                            }
                        }
                        sd.direct_control_system.process_keyboard(&event);
                    },
                    WindowEvent::MouseInput { state, button, .. } => {
                        if state == ElementState::Pressed {
                            match button {
                                MouseButton::Left => {
                                    if sd
                                        .player_position_system
                                        .get_target_block(
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
                                        .is_some()
                                    {
                                        // TODO Handle with script
                                        use bincode::{
                                            Decode,
                                            Encode,
                                        };
                                        use voxbrix_common::entity::{
                                            action::Action,
                                            chunk::Chunk,
                                        };

                                        let (position, direction) =
                                            sd.player_position_system.position_direction(
                                                &sd.position_ac,
                                                &sd.orientation_ac,
                                            );

                                        #[derive(Encode, Decode)]
                                        pub struct RemoveBlock {
                                            chunk: Chunk,
                                            offset: [f32; 3],
                                            direction: [f32; 3],
                                        }

                                        sd.actions_packer.add_action(
                                            Action(0),
                                            sd.snapshot,
                                            RemoveBlock {
                                                chunk: position.chunk,
                                                offset: position.offset.into(),
                                                direction: direction.into(),
                                            },
                                        );
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
                                        let mut block = block.into_coords().map(|u| u as i32);
                                        block[axis] += direction;

                                        if Block::from_chunk_offset(chunk, block).is_some() {
                                            // TODO Handle with script
                                            use bincode::{
                                                Decode,
                                                Encode,
                                            };
                                            use voxbrix_common::entity::{
                                                action::Action,
                                                block_class::BlockClass,
                                                chunk::Chunk,
                                            };

                                            let (position, direction) =
                                                sd.player_position_system.position_direction(
                                                    &sd.position_ac,
                                                    &sd.orientation_ac,
                                                );

                                            #[derive(Encode, Decode)]
                                            pub struct PlaceBlock {
                                                chunk: Chunk,
                                                offset: [f32; 3],
                                                direction: [f32; 3],
                                                block_class: BlockClass,
                                            }

                                            sd.actions_packer.add_action(
                                                Action(1),
                                                sd.snapshot,
                                                PlaceBlock {
                                                    chunk: position.chunk,
                                                    offset: position.offset.into(),
                                                    direction: direction.into(),
                                                    block_class: sd
                                                        .block_class_label_map
                                                        .get("grass")
                                                        .unwrap(),
                                                },
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
