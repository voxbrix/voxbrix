use crate::{
    component::{
        actor::class::ClassActorComponent,
        actor_class::health::HealthActorClassComponent,
    },
    resource::{
        interface::Interface,
        player_actor::PlayerActor,
    },
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct HUDSystem;

impl System for HUDSystem {
    type Data<'a> = HUDSystemData<'a>;
}

#[derive(SystemData)]
pub struct HUDSystemData<'a> {
    player_actor: &'a PlayerActor,
    interface: &'a Interface,
    class_ac: &'a ClassActorComponent,
    health_acc: &'a HealthActorClassComponent,
}

impl HUDSystemData<'_> {
    pub fn run(self) {
        self.interface.add_element(|ctx| {
            if let Some(health) = self
                .class_ac
                .get(&self.player_actor.0)
                .and_then(|class| self.health_acc.get(class, &self.player_actor.0))
                .and_then(|health| health.ratio())
            {
                egui::Area::new("hud_resources".into())
                    .default_size([300.0, 100.0])
                    .anchor(egui::Align2::LEFT_BOTTOM, [16.0, -16.0])
                    .show(ctx, |ui| {
                        ui.add(egui::ProgressBar::new(health).fill(egui::Color32::RED))
                    });
            }
        });
    }
}
