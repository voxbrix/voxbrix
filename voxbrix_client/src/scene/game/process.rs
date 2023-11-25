use super::Transition;
use crate::{
    scene::game::data::GameSharedData,
    system::{
        chunk_render_pipeline::Procedure,
        render::{
            output_thread::OutputBundle,
            Renderer,
        },
    },
};
use rayon::prelude::*;
use std::time::Instant;

pub struct Process<'a> {
    pub shared_data: &'a mut GameSharedData,
    pub output_bundle: OutputBundle,
}

impl Process<'_> {
    pub fn run(self) -> Transition {
        let Process {
            shared_data: sd,
            output_bundle,
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

        let player_chunk = sd
            .position_ac
            .get(&sd.player_actor)
            .expect("player Actor must exist")
            .chunk;

        sd.chunk_presence_system.process(
            sd.player_chunk_view_radius,
            &sd.player_actor,
            &sd.position_ac,
            &mut sd.status_cc,
            |chunk| {
                sd.class_bc.remove_chunk(&chunk);
                sd.sky_light_bc.remove_chunk(&chunk);
                sd.block_render_system.remove_chunk(&chunk);
                sd.chunk_render_pipeline_system
                    .chunk_removed(chunk, |chunk| sd.class_bc.get_chunk(chunk).is_some());
            },
        );

        sd.chunk_render_pipeline_system
            .compute_next(player_chunk, |context| {
                match context.procedure {
                    Procedure::ComputeSkyLight => {
                        sd.sky_light_system.compute_chunks(
                            context,
                            &sd.class_bc,
                            &sd.opacity_bcc,
                            &mut sd.sky_light_bc,
                        );
                    },
                    Procedure::BuildPolygons => {
                        sd.block_render_system.compute_chunks(
                            context,
                            &sd.class_bc,
                            &sd.model_bcc,
                            &sd.builder_bmc,
                            &sd.culling_bmc,
                            &sd.sky_light_bc,
                        )
                    },
                }
            });

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

        sd.interface_system
            .start(sd.render_system.output_thread().window());

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
            &mut sd.animation_state_ac,
        );

        sd.render_system.start_render(output_bundle);

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
