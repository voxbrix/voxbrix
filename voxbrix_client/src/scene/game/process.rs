use super::Transition;
use crate::{
    resource::{
        interface::Interface,
        render_pool::{
            RenderPool,
            Renderer,
        },
    },
    system::{
        actor_render::ActorRenderSystem,
        block_render::BlockRenderSystem,
        chunk_presence::ChunkPresenceSystem,
        interface_render::InterfaceRenderSystem,
        inventory_window::InventoryWindowSystem,
        movement_interpolation::MovementInterpolationSystem,
        player_control::PlayerControlSystem,
        player_position::PlayerPositionSystem,
        target_block_highlight::TargetBlockHightlightSystem,
        update_render_pool::UpdateRenderPoolSystem,
    },
    window::Frame,
};
use rayon::prelude::*;
use voxbrix_common::resource::process_timer::ProcessTimer;
use voxbrix_world::World;

pub struct Process<'a> {
    pub world: &'a mut World,
    pub frame: Frame,
}

impl Process<'_> {
    pub fn run(self) -> Transition {
        let Process { world, mut frame } = self;

        world.get_resource_mut::<ProcessTimer>().record_next();

        world.get_data::<ChunkPresenceSystem>().run();

        world.get_data::<PlayerPositionSystem>().run();

        world.get_data::<PlayerControlSystem>().run();

        world.get_data::<MovementInterpolationSystem>().run();

        world.get_resource_mut::<Interface>().initialize(&mut frame);

        world.get_data::<InventoryWindowSystem>().run();

        world.get_data::<UpdateRenderPoolSystem>().run(frame);

        let mut render_pool = world.take_resource::<RenderPool>();

        let (mut block_rd, mut target_block_hl, mut actor_rd, mut interface_rd) = world
            .get_data::<(
                BlockRenderSystem,
                TargetBlockHightlightSystem,
                ActorRenderSystem,
                InterfaceRenderSystem,
            )>();

        let render_systems: [&mut (dyn FnMut(Renderer) + Send); 4] = [
            &mut |renderer| {
                block_rd.run(renderer);
            },
            &mut |renderer| {
                target_block_hl.run(renderer);
            },
            &mut |renderer| {
                actor_rd.run(renderer);
            },
            &mut |renderer| {
                // Interface must be the last because only the last renderer has UI renderer:
                interface_rd.run(renderer);
            },
        ];

        render_pool
            .get_renderers::<4>()
            .into_iter()
            .zip(render_systems.into_iter())
            .par_bridge()
            .for_each(|(renderer, system)| system(renderer));

        render_pool.finish_render();
        world.return_resource(render_pool);

        Transition::None
    }
}
