use crate::entity::{
    action::Action,
    actor::Actor,
    block::Block,
    block_class::BlockClass,
    chunk::{
        Chunk,
        Dimension,
        DimensionKind,
    },
};

impl From<server_loop_api::Block> for Block {
    fn from(value: server_loop_api::Block) -> Self {
        Self::from_usize(value.as_usize().into()).expect("incorrect block passed from script")
    }
}

impl From<Block> for server_loop_api::Block {
    fn from(value: Block) -> Self {
        Self::from_usize(value.as_usize())
    }
}

impl From<server_loop_api::DimensionKind> for DimensionKind {
    fn from(value: server_loop_api::DimensionKind) -> Self {
        Self(value.0)
    }
}

impl From<DimensionKind> for server_loop_api::DimensionKind {
    fn from(value: DimensionKind) -> Self {
        Self(value.0)
    }
}

impl From<server_loop_api::Dimension> for Dimension {
    fn from(value: server_loop_api::Dimension) -> Self {
        Self {
            kind: value.kind.into(),
            phase: value.phase,
        }
    }
}

impl From<Dimension> for server_loop_api::Dimension {
    fn from(value: Dimension) -> Self {
        Self {
            kind: value.kind.into(),
            phase: value.phase,
        }
    }
}

impl From<server_loop_api::Chunk> for Chunk {
    fn from(value: server_loop_api::Chunk) -> Self {
        Self {
            position: value.position,
            dimension: value.dimension.into(),
        }
    }
}

impl From<Chunk> for server_loop_api::Chunk {
    fn from(value: Chunk) -> Self {
        Self {
            position: value.position,
            dimension: value.dimension.into(),
        }
    }
}

impl From<server_loop_api::BlockClass> for BlockClass {
    fn from(value: server_loop_api::BlockClass) -> Self {
        Self(value.0.into())
    }
}

impl From<server_loop_api::Action> for Action {
    fn from(value: server_loop_api::Action) -> Self {
        Self(value.0)
    }
}

impl From<Action> for server_loop_api::Action {
    fn from(value: Action) -> Self {
        Self(value.0)
    }
}

impl From<server_loop_api::Actor> for Actor {
    fn from(value: server_loop_api::Actor) -> Self {
        Self(value.0)
    }
}

impl From<Actor> for server_loop_api::Actor {
    fn from(value: Actor) -> Self {
        Self(value.0)
    }
}
