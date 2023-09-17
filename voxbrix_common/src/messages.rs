use crate::entity::{
    actor::Actor,
    state_component::StateComponent,
};
use nohash_hasher::IntMap;
use serde::{
    Deserialize,
    Serialize,
};
use std::mem;

pub mod client;
pub mod server;

/// State container.
/// The components supposed to be transfered in delta manner,
/// meaining that only changed components are in the map.
#[derive(Serialize, Deserialize)]
pub struct State<'a> {
    #[serde(borrow)]
    components: IntMap<StateComponent, &'a [u8]>,
}

impl State<'static> {
    pub fn new() -> Self {
        Self {
            components: IntMap::default(),
        }
    }
}

impl<'a> State<'a> {
    pub fn insert(&mut self, component: StateComponent, data: &'a [u8]) {
        self.components.insert(component, data);
    }

    pub fn clear(mut self) -> State<'static> {
        self.components.clear();

        // Safety: all references are removed in the previous step.
        unsafe { mem::transmute::<State<'a>, State<'static>>(self) }
    }

    pub fn get_component(&self, component: &StateComponent) -> Option<&'a [u8]> {
        self.components.get(component).copied()
    }
}

pub struct StatePacker {
    components: IntMap<StateComponent, (bool, Vec<u8>)>,
    state: Option<State<'static>>,
}

impl StatePacker {
    pub fn new() -> Self {
        Self {
            components: IntMap::default(),
            state: Some(State::new()),
        }
    }

    pub fn get_component_buffer(&mut self, component: StateComponent) -> &mut Vec<u8> {
        if self.components.get(&component).is_none() {
            self.components.insert(component, (false, Vec::new()));
        }

        let (is_packed, buffer) = self.components.get_mut(&component).unwrap();

        *is_packed = true;

        buffer
    }

    pub fn pack_state(&mut self, mut pack_fn: impl FnMut(State) -> State) {
        let mut state = self.state.take().unwrap();

        let extend_iter = self
            .components
            .iter()
            .filter_map(|(component, (is_packed, buffer))| {
                is_packed.then_some((*component, buffer.as_slice()))
            });

        state.components.extend(extend_iter);

        self.state = Some(pack_fn(state).clear());

        for (_, (is_packed, buffer)) in self.components.iter_mut() {
            *is_packed = false;
            buffer.clear();
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ActorState<T> {
    pub full: Vec<(Actor, T)>,
    pub change: Vec<(Actor, Option<T>)>,
}
