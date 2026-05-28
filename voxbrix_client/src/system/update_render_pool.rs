use crate::{
    component::{
        actor::{
            class::ClassActorComponent,
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
        },
        actor_class::sight::SightActorClassComponent,
    },
    resource::{
        interface_state::InterfaceState,
        player_actor::PlayerActor,
        render_pool::{
            CameraUpdate,
            RenderPool,
        },
    },
    window::Frame,
};
use voxbrix_common::resource::process_timer::ProcessTimer;
use voxbrix_world::{
    System,
    SystemData,
};

pub struct UpdateRenderPoolSystem;

impl System for UpdateRenderPoolSystem {
    type Data<'a> = UpdateRenderPoolSystemData<'a>;
}

#[derive(SystemData)]
pub struct UpdateRenderPoolSystemData<'a> {
    render_pool: &'a mut RenderPool,
    process_timer: &'a ProcessTimer,
    player_actor: &'a PlayerActor,
    class_ac: &'a ClassActorComponent,
    position_ac: &'a PositionActorComponent,
    orientation_ac: &'a OrientationActorComponent,
    sight_acc: &'a SightActorClassComponent,
    interface_state: &'a mut InterfaceState,
}

impl UpdateRenderPoolSystemData<'_> {
    pub fn run(self, frame: Frame) {
        if self.interface_state.inventory_open && !self.interface_state.cursor_visible {
            self.render_pool.cursor_visibility(true);
            self.interface_state.cursor_visible = true;
        } else if !self.interface_state.inventory_open && self.interface_state.cursor_visible {
            self.render_pool.cursor_visibility(false);
            self.interface_state.cursor_visible = false;
        }

        let player_position = self
            .position_ac
            .get(&self.player_actor.0)
            .expect("player position is undefined");

        let player_orientation = self
            .orientation_ac
            .get(&self.player_actor.0)
            .expect("player orientation is undefined");

        let player_offset = if let Some(player_class) = self.class_ac.get(&self.player_actor.0) {
            let player_sight = self.sight_acc.get(player_class, &self.player_actor.0);
            player_position.offset + player_sight.view_offset
        } else {
            player_position.offset
        };

        self.render_pool.update_camera(CameraUpdate {
            chunk: player_position.chunk.position,
            offset: player_offset,
            view_direction: player_orientation.forward(),
            dt: self.process_timer.elapsed(),
        });

        self.render_pool.start_render(frame);
    }
}
