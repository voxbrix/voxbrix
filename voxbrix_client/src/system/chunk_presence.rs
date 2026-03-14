use crate::component::{
    actor::position::PositionActorComponent,
    block::class::ClassBlockComponent,
    chunk::{
        render_data::{
            BlkRenderDataChunkComponent,
            EnvRenderDataChunkComponent,
        },
        sky_light_data::SkyLightDataChunkComponent,
    },
};
use voxbrix_common::{
    component::{
        block::sky_light::SkyLightBlockComponent,
        chunk::status::StatusChunkComponent,
        dimension_kind::player_chunk_view::PlayerChunkViewDimensionKindComponent,
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
    position_ac: &'a PositionActorComponent,
    player_chunk_view_dkc: &'a PlayerChunkViewDimensionKindComponent,
    status_cc: &'a mut StatusChunkComponent,
    class_bc: &'a mut ClassBlockComponent,
    sky_light_bc: &'a mut SkyLightBlockComponent,
    blk_render_data_cc: &'a mut BlkRenderDataChunkComponent,
    env_render_data_cc: &'a mut EnvRenderDataChunkComponent,
    sky_light_data_cc: &'a mut SkyLightDataChunkComponent,
}

impl ChunkPresenceSystemData<'_> {
    pub fn run(self) {
        let should_exist = |chunk: &Chunk| {
            self.position_ac.player_chunks().any(|ctl_chunk| {
                self.player_chunk_view_dkc
                    .get(&ctl_chunk.dimension.kind)
                    .to_chunk_radius(ctl_chunk)
                    .is_within(chunk)
            })
        };

        self.status_cc.retain(|chunk, _| {
            let retain = should_exist(chunk);
            if !retain {
                self.class_bc.remove_chunk(chunk);
                self.sky_light_bc.remove_chunk(chunk);
                self.blk_render_data_cc.remove_chunk(chunk);
                self.env_render_data_cc.remove_chunk(chunk);
                self.sky_light_data_cc.remove_chunk(chunk);
            }
            retain
        });
    }
}
