use crate::component::{
    actor::position::PositionActorComponent,
    block::class::ClassBlockComponent,
    player::{
        actions_packer::ActionsPackerPlayerComponent,
        actor::ActorPlayerComponent,
        chunk_view::ChunkViewPlayerComponent,
    },
};
use log::debug;
use server_loop_api::{
    ActionInput,
    GetTargetBlockRequest,
    GetTargetBlockResponse,
    SetClassOfBlockRequest,
};
use std::mem;
use voxbrix_common::{
    component::{
        actor::position::Position,
        block_class::collision::CollisionBlockClassComponent,
    },
    entity::{
        actor::Actor,
        block::BLOCKS_IN_CHUNK_EDGE,
        block_class::BlockClass,
        snapshot::Snapshot,
    },
    pack::{
        self,
    },
    script_registry::{
        self,
        ScriptData,
        ScriptRegistry,
        ScriptRegistryBuilder,
    },
    system::position,
    LabelLibrary,
};
use wasmtime::Caller;

pub struct ScriptSharedDataRef<'a> {
    pub snapshot: Snapshot,
    pub label_library: &'a LabelLibrary,
    pub actor_pc: &'a ActorPlayerComponent,
    pub actions_packer_pc: &'a mut ActionsPackerPlayerComponent,
    pub chunk_view_pc: &'a ChunkViewPlayerComponent,
    pub position_ac: &'a PositionActorComponent,
    pub class_bc: &'a mut ClassBlockComponent,
    pub collision_bcc: &'a CollisionBlockClassComponent,
}

impl<'a> ScriptSharedDataRef<'a> {
    pub fn into_static(self) -> ScriptSharedData {
        // SAFETY: the resulting ScriptSharedData can only be used via unsafe methods.
        ScriptSharedData(unsafe {
            mem::transmute::<ScriptSharedDataRef<'a>, ScriptSharedDataRef<'static>>(self)
        })
    }
}

pub struct ScriptSharedData(ScriptSharedDataRef<'static>);

impl ScriptSharedData {
    pub unsafe fn get<'a>(&'a self) -> &'a ScriptSharedDataRef<'a> {
        mem::transmute::<&'a ScriptSharedDataRef<'static>, &'a ScriptSharedDataRef<'a>>(&self.0)
    }

    pub unsafe fn get_mut<'a>(&'a mut self) -> &'a mut ScriptSharedDataRef<'a> {
        mem::transmute::<&'a mut ScriptSharedDataRef<'static>, &'a mut ScriptSharedDataRef<'a>>(
            &mut self.0,
        )
    }
}

// Try to make unsafe blocks only output owned types.
pub fn setup_script_registry(
    mut registry: ScriptRegistryBuilder<ScriptSharedData>,
) -> ScriptRegistry<ScriptSharedData> {
    fn handle_panic(caller: Caller<ScriptData<ScriptSharedData>>, msg_ptr: u32, msg_len: u32) {
        let ptr = msg_ptr as usize;
        let len = msg_len as usize;
        let memory = caller.data().memory();
        let msg = std::str::from_utf8(&memory.data(&caller)[ptr .. ptr + len]).unwrap();

        panic!("script ended with panic: {}", msg);
    }

    registry.func_wrap("env", "handle_panic", handle_panic);

    fn log_message(caller: Caller<ScriptData<ScriptSharedData>>, msg_ptr: u32, msg_len: u32) {
        let ptr = msg_ptr as usize;
        let len = msg_len as usize;
        let memory = caller.data().memory();
        let msg = std::str::from_utf8(&memory.data(&caller)[ptr .. ptr + len]).unwrap();

        log::error!("{}", msg);
    }

    registry.func_wrap("env", "log_message", log_message);

    fn get_blocks_in_chunk_edge(_: Caller<ScriptData<ScriptSharedData>>) -> u32 {
        BLOCKS_IN_CHUNK_EDGE as u32
    }

    registry.func_wrap("env", "get_blocks_in_chunk_edge", get_blocks_in_chunk_edge);

    fn get_target_block(
        mut caller: Caller<ScriptData<ScriptSharedData>>,
        buf_ptr: u32,
        buf_len: u32,
    ) {
        let ptr = buf_ptr as usize;
        let len = buf_len as usize;
        let memory = caller.data().memory();
        let bytes = &memory.data(&caller)[ptr .. ptr + len];

        let (command, _) =
            pack::decode_from_slice::<GetTargetBlockRequest>(bytes).expect("invalid argument");

        let sd = unsafe { caller.data().shared().get() };

        let response = position::get_target_block(
            &Position {
                chunk: command.chunk.into(),
                offset: command.offset.into(),
            },
            command.direction.into(),
            |chunk, block| {
                sd.class_bc
                    .get_chunk(&chunk)
                    .map(|blocks| {
                        let class = blocks.get(block);
                        sd.collision_bcc.get(class).is_some()
                    })
                    .unwrap_or(false)
            },
        )
        .map(|(chunk, block, side)| {
            GetTargetBlockResponse {
                chunk: chunk.into(),
                block: block.into(),
                side: side as u8,
            }
        });

        script_registry::write_script_buffer(&mut caller, response);
    }

    registry.func_wrap("env", "get_target_block", get_target_block);

    fn set_class_of_block(
        mut caller: Caller<ScriptData<ScriptSharedData>>,
        buf_ptr: u32,
        buf_len: u32,
    ) {
        let ptr = buf_ptr as usize;
        let len = buf_len as usize;
        let memory = caller.data().memory();
        let bytes = &memory.data(&caller)[ptr .. ptr + len];

        let (command, _) =
            pack::decode_from_slice::<SetClassOfBlockRequest>(bytes).expect("invalid argument");

        let sd = unsafe { caller.data_mut().shared_mut().get_mut() };

        let Some(mut classes) = sd.class_bc.get_mut_chunk(&command.chunk.into()) else {
            debug!("changing nonexistent chunk");
            return;
        };

        classes.set(command.block.into(), command.block_class.into());
    }

    registry.func_wrap("env", "set_class_of_block", set_class_of_block);

    fn get_block_class_by_label(
        mut caller: Caller<ScriptData<ScriptSharedData>>,
        buf_ptr: u32,
        buf_len: u32,
    ) {
        let ptr = buf_ptr as usize;
        let len = buf_len as usize;
        let memory = caller.data().memory();
        let bytes = &memory.data(&caller)[ptr .. ptr + len];

        let (label, _) = pack::decode_from_slice::<&str>(bytes).expect("invalid argument");

        let sd = unsafe { caller.data().shared().get() };

        let response = sd.label_library.get::<BlockClass>(label);

        script_registry::write_script_buffer(&mut caller, response);
    }

    registry.func_wrap("env", "get_block_class_by_label", get_block_class_by_label);

    fn broadcast_action_local(
        mut caller: Caller<ScriptData<ScriptSharedData>>,
        buf_ptr: u32,
        buf_len: u32,
    ) {
        let ptr = buf_ptr as usize;
        let len = buf_len as usize;
        let memory = caller.data().memory();
        let mut bytes = mem::take(caller.data_mut().buffer());
        bytes.extend_from_slice(&memory.data(&caller)[ptr .. ptr + len]);

        let sd = unsafe { caller.data_mut().shared_mut().get_mut() };

        // TODO Instead of option, in the future we should have either actor or "acting position"
        // directly as an enum.
        let input: ActionInput = pack::decode_from_slice(&bytes)
            .expect("unable to decode action data")
            .0;

        let action_actor: Option<Actor> = input.actor.map(Into::into);

        let acting_position = sd
            .position_ac
            .get(&action_actor.expect("actor was not passed by the script"))
            .expect("acting actor has no position");

        for player in sd.chunk_view_pc.iter().filter_map(|(player, chunk_view)| {
            let position = sd.position_ac.get(sd.actor_pc.get(&player)?)?;

            position
                .chunk
                .radius(chunk_view.radius)
                .is_within(&acting_position.chunk)
                .then_some(())?;

            Some(player)
        }) {
            sd.actions_packer_pc
                .get_mut(&player)
                .expect("no action packer found for a player")
                .add_action(input.action.into(), sd.snapshot, (action_actor, input.data));
        }

        *caller.data_mut().buffer() = bytes;
    }

    registry.func_wrap("env", "broadcast_action_local", broadcast_action_local);

    registry.build()
}
