use crate::{
    component::{
        actor::{
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
        },
        block::class::ClassBlockComponent,
    },
    resource::{
        interface_state::InterfaceState,
        player_actor::PlayerActor,
        player_input::PlayerInput,
    },
    scene::game::Transition,
    window::{
        InputEvent,
        WindowEvent,
    },
};
use voxbrix_common::{
    component::block_class::collision::CollisionBlockClassComponent,
    entity::{
        action::Action,
        block::Block,
        snapshot::ClientSnapshot,
    },
    messages::ClientActionsPacker,
    system::position::get_target_block,
    LabelLibrary,
};
use voxbrix_world::{
    System,
    SystemData,
    World,
};
use winit::event::{
    DeviceEvent,
    ElementState,
    MouseButton,
};

pub struct LocalInput<'a> {
    pub world: &'a mut World,
    pub event: InputEvent,
}

impl LocalInput<'_> {
    pub fn run(self) -> Transition {
        let LocalInput { world, event } = self;

        if world.get_resource_ref::<InterfaceState>().inventory_open {
            return Transition::None;
        }

        match event {
            InputEvent::DeviceEvent(event) => {
                match event {
                    DeviceEvent::MouseMotion {
                        delta: (horizontal, vertical),
                    } => {
                        world
                            .get_resource_mut::<PlayerInput>()
                            .process_mouse(horizontal as f32, vertical as f32);
                    },
                    _ => {},
                }
            },
            InputEvent::WindowEvent(event) => {
                match event {
                    WindowEvent::CloseRequested => {
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
                                        let int_st = world.get_resource_mut::<InterfaceState>();
                                        int_st.inventory_open = !int_st.inventory_open;
                                    },
                                    winit::keyboard::KeyCode::KeyF => {
                                        let snapshot = *world.get_resource_ref::<ClientSnapshot>();
                                        let actions_packer =
                                            world.get_resource_mut::<ClientActionsPacker>();

                                        actions_packer.add(Action(2), snapshot, ());
                                    },
                                    winit::keyboard::KeyCode::KeyL => {
                                        let snapshot = *world.get_resource_ref::<ClientSnapshot>();
                                        let actions_packer =
                                            world.get_resource_mut::<ClientActionsPacker>();

                                        actions_packer.add(Action(3), snapshot, ());
                                    },
                                    _ => {},
                                }
                            }
                        }
                        world
                            .get_resource_mut::<PlayerInput>()
                            .process_keyboard(&event);
                    },
                    WindowEvent::MouseInput { state, button, .. } => {
                        if state == ElementState::Pressed {
                            struct GetTargetBlockSystem;

                            impl System for GetTargetBlockSystem {
                                type Data<'a> = GetTargetBlockSystemData<'a>;
                            }

                            #[derive(SystemData)]
                            struct GetTargetBlockSystemData<'a> {
                                snapshot: &'a ClientSnapshot,
                                player_actor: &'a PlayerActor,
                                position_ac: &'a PositionActorComponent,
                                orientation_ac: &'a OrientationActorComponent,
                                class_bc: &'a ClassBlockComponent,
                                collision_bcc: &'a CollisionBlockClassComponent,
                                actions_packer: &'a mut ClientActionsPacker,
                                label_library: &'a LabelLibrary,
                            }

                            let sd = world.get_data::<GetTargetBlockSystem>();

                            let func = || {
                                let actor = sd.player_actor.0;
                                let position = sd.position_ac.get(&actor)?;
                                let orientation = sd.orientation_ac.get(&actor)?;

                                let target = get_target_block(
                                    &position,
                                    orientation.forward(),
                                    |chunk, block| {
                                        sd.class_bc
                                            .get_chunk(&chunk)
                                            .map(|blocks| {
                                                let class = blocks.get(block);
                                                sd.collision_bcc.get(class).is_some()
                                            })
                                            .unwrap_or(false)
                                    },
                                )?;

                                Some((position, orientation, target))
                            };

                            match button {
                                MouseButton::Left => {
                                    if let Some((position, orientation, _)) = func() {
                                        // TODO Handle with script
                                        use serde::{
                                            Deserialize,
                                            Serialize,
                                        };
                                        use voxbrix_common::entity::{
                                            action::Action,
                                            chunk::Chunk,
                                        };

                                        let direction = orientation.forward();

                                        #[derive(Serialize, Deserialize)]
                                        pub struct RemoveBlock {
                                            chunk: Chunk,
                                            offset: [f32; 3],
                                            direction: [f32; 3],
                                        }

                                        sd.actions_packer.add(
                                            Action(0),
                                            *sd.snapshot,
                                            RemoveBlock {
                                                chunk: position.chunk,
                                                offset: position.offset.into(),
                                                direction: direction.into(),
                                            },
                                        );
                                    }
                                },
                                MouseButton::Right => {
                                    if let Some((position, orientation, (chunk, block, side))) =
                                        func()
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
                                            use serde::{
                                                Deserialize,
                                                Serialize,
                                            };
                                            use voxbrix_common::entity::{
                                                action::Action,
                                                block_class::BlockClass,
                                                chunk::Chunk,
                                            };

                                            let direction = orientation.forward();

                                            #[derive(Serialize, Deserialize)]
                                            pub struct PlaceBlock {
                                                chunk: Chunk,
                                                offset: [f32; 3],
                                                direction: [f32; 3],
                                                block_class: BlockClass,
                                            }

                                            sd.actions_packer.add(
                                                Action(1),
                                                *sd.snapshot,
                                                PlaceBlock {
                                                    chunk: position.chunk,
                                                    offset: position.offset.into(),
                                                    direction: direction.into(),
                                                    block_class: sd
                                                        .label_library
                                                        .get::<BlockClass>("grass")
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
