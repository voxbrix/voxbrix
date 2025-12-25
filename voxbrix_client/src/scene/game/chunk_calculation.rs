use super::Transition;
use crate::{
    resource::chunk_calculation_data::ChunkCalculationData,
    system::{
        block_environment_model::BlockEnvironmentModelSystem,
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
        let turn = self.world.get_resource_ref::<ChunkCalculationData>().turn;

        let work_array = [
            |world: &mut World| {
                world.get_data::<SkyLightSystem>().run(BLOCKS_IN_CHUNK);
            },
            |world: &mut World| {
                world.get_data::<BlockModelSystem>().run();
            },
            |world: &mut World| {
                world.get_data::<BlockEnvironmentModelSystem>().run();
            },
        ];

        work_array[turn](self.world);

        self.world.get_resource_mut::<ChunkCalculationData>().turn = (turn + 1) % work_array.len();

        Transition::None
    }
}
