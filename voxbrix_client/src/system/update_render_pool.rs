use crate::{
    component::actor::{
        orientation::OrientationActorComponent,
        position::PositionActorComponent,
    },
    resource::{
        interface_state::InterfaceState,
        player_actor::PlayerActor,
        render_pool::RenderPool,
    },
    window::Frame,
};
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
    player_actor: &'a PlayerActor,
    position_ac: &'a PositionActorComponent,
    orientation_ac: &'a OrientationActorComponent,
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

        self.render_pool.camera_mut().update_position(
            player_position.chunk.position,
            player_position.offset.into(),
            player_orientation.forward().into(),
        );

        self.render_pool.start_render(frame);
    }
}
