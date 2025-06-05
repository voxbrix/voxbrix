use super::Transition;
use crate::{
    resource::chunk_calculation_data::ChunkCalculationData,
    system::{
        block_model::BlockModelSystem,
        sky_light::SkyLightSystem,
    },
};
use voxbrix_common::entity::block::BLOCKS_IN_CHUNK;
use voxbrix_world::World;

pub struct ChunkCalculation<'a> {
    pub world: &'a mut World,
}
impl ChunkCalculation<'_> {
    pub fn run(self) -> Transition {
        let mut turn = self.world.get_resource_ref::<ChunkCalculationData>().turn;

        turn = match turn {
            0 => {
                self.world.get_data::<SkyLightSystem>().run(BLOCKS_IN_CHUNK);

                1
            },
            1 => {
                self.world.get_data::<BlockModelSystem>().run();

                0
            },
            _ => unreachable!(),
        };

        self.world.get_resource_mut::<ChunkCalculationData>().turn = turn;

        Transition::None
    }
}
