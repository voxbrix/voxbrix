use super::Transition;
use crate::{
    component::chunk::render_priority::Action,
    scene::game::data::GameSharedData,
    system::render::{
        output_thread::OutputBundle,
        Renderer,
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
                sd.render_priority_cc.chunk_removed(&chunk);
            },
        );

        for (chunk, action) in sd.render_priority_cc.drain_queue() {
            match action {
                Action::Add | Action::Update => {
                    sd.sky_light_system.add_chunk(chunk);
                },
                Action::Remove => {
                    sd.block_render_system.add_chunk(chunk);
                },
            }
        }

        let build_chunk_queue = sd
            .block_render_system
            .fill_chunk_queue(&sd.render_priority_cc, player_chunk);
        let sky_light_queue = sd
            .sky_light_system
            .fill_chunk_queue(&sd.render_priority_cc, player_chunk);

        enum Procedure {
            None,
            BuildChunk,
            ComputeSkyLight,
        }

        let action = match (build_chunk_queue.first(), sky_light_queue.first()) {
            (None, None) => Procedure::None,
            (Some(_), None) => Procedure::BuildChunk,
            (None, Some(_)) => Procedure::ComputeSkyLight,
            (Some((_, player_dist_1, priority_1)), Some((_, player_dist_2, priority_2))) => {
                let ordering = priority_1
                    .cmp(priority_2)
                    .reverse()
                    .then(player_dist_1.cmp(player_dist_2));
                // Ones not already calculated go first
                // Less player distance is the priority otherwise
                if ordering.is_le() {
                    Procedure::BuildChunk
                } else {
                    Procedure::ComputeSkyLight
                }
            },
        };

        match action {
            Procedure::None => {},
            Procedure::BuildChunk => {
                let chunk_opt = sd.block_render_system.build_next_chunk(
                    &sd.class_bc,
                    &sd.model_bcc,
                    &sd.builder_bmc,
                    &sd.culling_bmc,
                    &sd.sky_light_bc,
                );

                sd.render_priority_cc.finish_chunks(chunk_opt.into_iter())
            },
            Procedure::ComputeSkyLight => {
                sd.sky_light_system.compute_queued(
                    &sd.class_bc,
                    &sd.opacity_bcc,
                    &mut sd.sky_light_bc,
                );

                for chunk in sd.sky_light_system.drain_processed_chunks() {
                    sd.block_render_system.add_chunk(chunk);
                }
            },
        }

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
