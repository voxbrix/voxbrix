use crate::component::{
    actor::{
        class::ClassActorComponent,
        orientation::OrientationActorComponent,
        position::PositionActorComponent,
        target_orientation::TargetOrientationActorComponent,
        target_position::TargetPositionActorComponent,
        velocity::VelocityActorComponent,
        TargetQueue,
    },
    actor_class::model::ModelActorClassComponent,
};
use std::time::Instant;
use voxbrix_common::{
    component::actor::{
        orientation::Orientation,
        position::Position,
    },
    entity::snapshot::ClientSnapshot,
    messages::{
        client::ServerState,
        ClientActionsPacker,
        UpdatesUnpacker,
    },
};
use voxbrix_world::{
    System,
    SystemData,
};

pub enum Error {
    /// Unable to unpack updates.
    UnpackError,
}

pub struct ServerUpdatesSystem;

impl System for ServerUpdatesSystem {
    type Data<'a> = ServerUpdatesSystemData<'a>;
}

#[derive(SystemData)]
pub struct ServerUpdatesSystemData<'a> {
    snapshot: &'a ClientSnapshot,
    class_ac: &'a mut ClassActorComponent,
    model_acc: &'a mut ModelActorClassComponent,
    position_ac: &'a mut PositionActorComponent,
    target_position_ac: &'a mut TargetPositionActorComponent,
    orientation_ac: &'a mut OrientationActorComponent,
    target_orientation_ac: &'a mut TargetOrientationActorComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    actions_packer: &'a mut ClientActionsPacker,
    updates_unpacker: &'a mut UpdatesUnpacker,
}

impl ServerUpdatesSystemData<'_> {
    pub fn run(&mut self, data: &ServerState) -> Result<(), Error> {
        let current_time = Instant::now();
        let new_lss = data.snapshot;
        let updates = self
            .updates_unpacker
            .unpack(&data.updates)
            .map_err(|_| Error::UnpackError)?;

        self.class_ac.unpack(&updates);
        self.model_acc.unpack(&updates);
        self.velocity_ac.unpack(&updates);
        self.orientation_ac.unpack_target(&updates);
        self.target_orientation_ac.unpack_convert(
            &updates,
            |actor, previous, orientation: Orientation| {
                let current_value = if let Some(p) = self.orientation_ac.get(&actor) {
                    *p
                } else {
                    self.orientation_ac
                        .insert(actor, orientation, *self.snapshot);
                    orientation
                };

                TargetQueue::from_previous(
                    previous,
                    current_value,
                    orientation,
                    current_time,
                    new_lss,
                )
            },
        );
        self.position_ac.unpack_target(&updates);
        self.target_position_ac
            .unpack_convert(&updates, |actor, previous, position: Position| {
                let current_value = if let Some(p) = self.position_ac.get(&actor) {
                    *p
                } else {
                    self.position_ac.insert(actor, position, *self.snapshot);
                    position
                };

                TargetQueue::from_previous(previous, current_value, position, current_time, new_lss)
            });

        self.actions_packer
            .confirm_snapshot(data.last_client_snapshot);
        self.position_ac.confirm_snapshot(data.last_client_snapshot);

        Ok(())
    }
}
