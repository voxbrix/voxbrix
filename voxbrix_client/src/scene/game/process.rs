use super::Transition;
use crate::{
    scene::game::data::GameSharedData,
    system::render::Renderer,
    window::Frame,
};
use rayon::prelude::*;
use std::time::Instant;

pub struct Process<'a> {
    pub shared_data: &'a mut GameSharedData,
    pub frame: Frame,
}

impl Process<'_> {
    pub fn run(self) -> Transition {
        let Process {
            shared_data: sd,
            mut frame,
        } = self;

        if sd.inventory_open && !sd.cursor_visible {
            sd.render_system.cursor_visibility(true);
            sd.cursor_visible = true;
        } else if !sd.inventory_open && sd.cursor_visible {
            sd.render_system.cursor_visibility(false);
            sd.cursor_visible = false;
        }

        let now = Instant::now();
        let elapsed = now.saturating_duration_since(sd.last_process_time);
        sd.last_process_time = now;

        sd.chunk_presence_system.process(
            sd.player_chunk_view_radius,
            &sd.player_actor,
            &sd.position_ac,
            &mut sd.status_cc,
            |chunk| {
                sd.class_bc.remove_chunk(&chunk);
                sd.sky_light_bc.remove_chunk(&chunk);
                sd.block_render_system.remove_chunk(&chunk);
                sd.sky_light_system.remove_chunk(&chunk);
            },
        );

        sd.player_position_system.process(
            elapsed,
            &sd.class_bc,
            &sd.collision_bcc,
            &mut sd.position_ac,
            &sd.velocity_ac,
            sd.snapshot,
        );
        sd.direct_control_system.process(
            elapsed,
            &mut sd.velocity_ac,
            &mut sd.orientation_ac,
            sd.snapshot,
        );
        sd.movement_interpolation_system.process(
            &mut sd.target_position_ac,
            &mut sd.target_orientation_ac,
            &mut sd.position_ac,
            &mut sd.orientation_ac,
            sd.snapshot,
        );

        let target = sd.player_position_system.get_target_block(
            &sd.position_ac,
            &sd.orientation_ac,
            |chunk, block| {
                // TODO: better targeting collision?
                sd.class_bc
                    .get_chunk(&chunk)
                    .map(|blocks| {
                        let class = blocks.get(block);
                        sd.collision_bcc.get(class).is_some()
                    })
                    .unwrap_or(false)
            },
        );

        sd.block_render_system.build_target_highlight(target);

        sd.interface_system.start(&mut frame);

        sd.interface_system.add_interface(|ctx| {
            egui::Window::new("Inventory")
                .open(&mut sd.inventory_open)
                .show(ctx, |ui| {
                    ui.label("Hello World!");
                });
        });

        sd.render_system.update(&sd.position_ac, &sd.orientation_ac);
        sd.actor_render_system.update(
            sd.player_actor,
            &sd.class_ac,
            &sd.position_ac,
            &sd.velocity_ac,
            &sd.orientation_ac,
            &sd.model_acc,
            &sd.builder_amc,
            &sd.sky_light_bc,
            &mut sd.animation_state_ac,
        );

        sd.render_system.start_render(frame);

        let render_systems: [&mut (dyn FnMut(Renderer) + Send); 3] = [
            &mut |renderer| {
                sd.block_render_system.render(renderer);
            },
            &mut |renderer| {
                sd.actor_render_system.render(renderer);
            },
            &mut |renderer| {
                sd.interface_system.render(renderer);
            },
        ];

        sd.render_system
            .get_renderers::<3>()
            .into_iter()
            .zip(render_systems.into_iter())
            .par_bridge()
            .for_each(|(renderer, system)| system(renderer));

        sd.render_system.finish_render();

        Transition::None
    }
}
