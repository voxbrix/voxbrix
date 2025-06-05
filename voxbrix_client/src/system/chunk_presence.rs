use crate::{
    component::{
        actor::position::PositionActorComponent,
        block::class::ClassBlockComponent,
        chunk::{
            render_data::RenderDataChunkComponent,
            sky_light_data::SkyLightDataChunkComponent,
        },
    },
    resource::player_chunk_view_radius::PlayerChunkViewRadius,
};
use voxbrix_common::{
    component::{
        block::sky_light::SkyLightBlockComponent,
        chunk::status::StatusChunkComponent,
    },
    entity::chunk::Chunk,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct ChunkPresenceSystem;

impl System for ChunkPresenceSystem {
    type Data<'a> = ChunkPresenceSystemData<'a>;
}

#[derive(SystemData)]
pub struct ChunkPresenceSystemData<'a> {
    radius: &'a PlayerChunkViewRadius,
    position_ac: &'a PositionActorComponent,
    status_cc: &'a mut StatusChunkComponent,
    class_bc: &'a mut ClassBlockComponent,
    sky_light_bc: &'a mut SkyLightBlockComponent,
    render_data_cc: &'a mut RenderDataChunkComponent,
    sky_light_data_cc: &'a mut SkyLightDataChunkComponent,
}

impl ChunkPresenceSystemData<'_> {
    pub fn run(self) {
        let should_exist = |chunk: &Chunk| {
            self.position_ac
                .player_chunks()
                .find(|ctl_chunk| ctl_chunk.radius(self.radius.0).is_within(chunk))
                .is_some()
        };

        self.status_cc.retain(|chunk, _| {
            let retain = should_exist(chunk);
            if !retain {
                self.class_bc.remove_chunk(&chunk);
                self.sky_light_bc.remove_chunk(&chunk);
                self.render_data_cc.remove_chunk(&chunk);
                self.sky_light_data_cc.remove_chunk(&chunk);
            }
            retain
        });
    }
}
