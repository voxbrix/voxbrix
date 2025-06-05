use crate::{
    entity::{
        action::Action,
        actor::Actor,
        snapshot::Snapshot,
        state_component::StateComponent,
    },
    pack::{
        self,
        UnpackError,
    },
};
use nohash_hasher::{
    IntMap,
    IntSet,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    collections::VecDeque,
    mem,
};

pub mod client;
pub mod server;

/// State container.
/// The components supposed to be transfered in delta manner,
/// meaining that only changed components are in the map.
#[derive(Serialize, Deserialize)]
pub struct StatePacked<'a>(&'a [u8]);

pub struct StateUnpacked<'a> {
    origin: &'a mut StateUnpacker,
    components: IntMap<StateComponent, &'a [u8]>,
}

impl<'a> StateUnpacked<'a> {
    pub fn get_component(&self, component: &StateComponent) -> Option<&'a [u8]> {
        self.components.get(component).copied()
    }
}

impl<'a> Drop for StateUnpacked<'a> {
    fn drop(&mut self) {
        let mut buffer = mem::take(&mut self.components);

        buffer.clear();

        // Safety: all references are removed in the previous step.
        let buffer = unsafe {
            mem::transmute::<IntMap<StateComponent, &'a [u8]>, IntMap<StateComponent, &'static [u8]>>(
                buffer,
            )
        };

        self.origin.buffer = buffer;
    }
}

#[derive(Default)]
pub struct StateUnpacker {
    buffer: IntMap<StateComponent, &'static [u8]>,
}

impl StateUnpacker {
    pub fn new() -> Self {
        Self {
            buffer: IntMap::default(),
        }
    }
}

impl StateUnpacker {
    pub fn unpack_state<'a>(
        &'a mut self,
        state: &'a StatePacked<'a>,
    ) -> Result<StateUnpacked<'a>, UnpackError> {
        let mut buffer = mem::take(&mut self.buffer);

        let (size, mut offset) = pack::decode_from_slice::<u64>(state.0).ok_or(UnpackError)?;

        let size: usize = size.try_into().map_err(|_| UnpackError)?;

        buffer.reserve(size);

        for _ in 0 .. size {
            let ((key, value), new_offset) =
                pack::decode_from_slice::<(StateComponent, &[u8])>(&state.0[offset ..])
                    .ok_or(UnpackError)?;

            offset += new_offset;

            buffer.insert(key, value);
        }

        let unpacked = StateUnpacked {
            origin: self,
            components: buffer,
        };

        Ok(unpacked)
    }
}

pub struct StatePacker {
    packed_components: IntSet<StateComponent>,
    components: IntMap<StateComponent, Vec<u8>>,
    to_be_cleared: bool,
    buffer: Vec<u8>,
}

impl StatePacker {
    pub fn new() -> Self {
        Self {
            packed_components: IntSet::default(),
            components: IntMap::default(),
            to_be_cleared: false,
            buffer: Vec::new(),
        }
    }

    pub fn get_component_buffer(&mut self, component: StateComponent) -> &mut Vec<u8> {
        if self.to_be_cleared {
            self.to_be_cleared = false;

            for component in self.packed_components.drain() {
                self.components.get_mut(&component).unwrap().clear();
            }
        }

        if self.components.get(&component).is_none() {
            self.components.insert(component, Vec::new());
        }

        let buffer = self.components.get_mut(&component).unwrap();

        self.packed_components.insert(component);

        buffer
    }

    pub fn pack_state<'a>(&'a mut self) -> StatePacked<'a> {
        let extend_iter = self
            .packed_components
            .iter()
            .map(|comp| self.components.get_key_value(comp).unwrap());

        let components_count = self.packed_components.len();

        self.buffer.clear();

        pack::encode_write(&(components_count as u64), &mut self.buffer);

        for component in extend_iter {
            pack::encode_write(&component, &mut self.buffer);
        }

        self.to_be_cleared = true;

        StatePacked(self.buffer.as_slice())
    }
}

#[derive(Deserialize)]
pub enum ActorStateUnpack<T> {
    Full(Vec<(Actor, T)>),
    Change(Vec<(Actor, Option<T>)>),
}

#[derive(Serialize)]
pub enum ActorStatePack<'a, T> {
    Full(&'a [(Actor, &'a T)]),
    Change(&'a [(Actor, Option<&'a T>)]),
}

#[derive(Serialize, Deserialize)]
pub struct ActionsPacked<'a>(&'a [u8]);

pub struct ActionsUnpacked<'a> {
    origin: &'a mut ActionsUnpacker,
    data: Vec<(Action, Snapshot, &'a [u8])>,
}

impl<'a> ActionsUnpacked<'a> {
    pub fn data(&self) -> &[(Action, Snapshot, &'a [u8])] {
        self.data.as_slice()
    }
}

impl<'a> Drop for ActionsUnpacked<'a> {
    fn drop(&mut self) {
        let mut buffer = mem::take(&mut self.data);

        buffer.clear();

        // Safety: all references are removed in the previous step.
        let buffer = unsafe {
            mem::transmute::<
                Vec<(Action, Snapshot, &'a [u8])>,
                Vec<(Action, Snapshot, &'static [u8])>,
            >(buffer)
        };

        self.origin.buffer = buffer;
    }
}

#[derive(Default)]
pub struct ActionsUnpacker {
    buffer: Vec<(Action, Snapshot, &'static [u8])>,
}

impl ActionsUnpacker {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }
}

impl ActionsUnpacker {
    pub fn unpack_actions<'a>(
        &'a mut self,
        actions: &'a ActionsPacked<'a>,
    ) -> Result<ActionsUnpacked<'a>, UnpackError> {
        let mut buffer = mem::take(&mut self.buffer);

        let (size, mut offset) = pack::decode_from_slice::<u64>(actions.0).ok_or(UnpackError)?;

        let size: usize = size.try_into().map_err(|_| UnpackError)?;

        buffer.reserve(size);

        for _ in 0 .. size {
            let ((action, snapshot, data), new_offset) =
                pack::decode_from_slice::<(Action, Snapshot, &[u8])>(&actions.0[offset ..])
                    .ok_or(UnpackError)?;

            offset += new_offset;

            buffer.push((action, snapshot, data));
        }

        let unpacked = ActionsUnpacked {
            origin: self,
            data: buffer,
        };

        Ok(unpacked)
    }
}

pub struct ActionsPacker {
    buffer: Vec<u8>,
    actions: VecDeque<(Snapshot, Action, usize)>,
    data: VecDeque<u8>,
}

impl ActionsPacker {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            actions: VecDeque::new(),
            data: VecDeque::new(),
        }
    }

    pub fn add_action(&mut self, action: Action, snapshot: Snapshot, data: impl Serialize) {
        let size = pack::encode_write(&data, &mut self.data);

        self.actions.push_back((snapshot, action, size));
    }

    fn remove_action(&mut self) {
        if let Some((_, _, size)) = self.actions.pop_front() {
            self.data.drain(.. size);
        }
    }

    pub fn confirm_snapshot(&mut self, snapshot: Snapshot) {
        loop {
            match self.actions.front() {
                Some((action_snapshot, _, _)) if *action_snapshot <= snapshot => {
                    self.remove_action();
                },
                _ => break,
            }
        }
    }

    pub fn pack_actions<'a>(&'a mut self) -> ActionsPacked<'a> {
        let mut data_cursor = 0;

        let data_slice: &_ = self.data.make_contiguous();

        let extend_iter = self
            .actions
            .iter()
            .copied()
            .map(|(snapshot, action, size)| {
                let read_from = data_cursor;
                data_cursor += size;
                (action, snapshot, &data_slice[read_from .. data_cursor])
            });

        let components_count = self.actions.len();

        self.buffer.clear();

        pack::encode_write(&(components_count as u64), &mut self.buffer);

        for element in extend_iter {
            pack::encode_write(&element, &mut self.buffer);
        }

        ActionsPacked(self.buffer.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack;

    #[test]
    fn test_actor_state_serde() {
        let initial = ActorStatePack::Full(&[(Actor(3), &"Actor3")]);
        let mut buffer = Vec::new();
        pack::encode_write(&initial, &mut buffer);
        let (control, _) = pack::decode_from_slice::<ActorStateUnpack<String>>(&buffer).unwrap();

        match (initial, control) {
            (ActorStatePack::Full(initial), ActorStateUnpack::Full(control)) => {
                assert_eq!(initial[0].0, control[0].0);
                assert_eq!(*initial[0].1, control[0].1.as_str());
            },
            _ => unreachable!(),
        }

        let initial = ActorStatePack::Full(&[
            (Actor(7), &"Actor7"),
            (Actor(1), &"Actor1"),
            (Actor(13), &"Actor13"),
        ]);
        let mut buffer = Vec::new();
        pack::encode_write(&initial, &mut buffer);
        let (control, _) = pack::decode_from_slice::<ActorStateUnpack<String>>(&buffer).unwrap();

        match (initial, control) {
            (ActorStatePack::Full(initial), ActorStateUnpack::Full(control)) => {
                for (initial, control) in initial.iter().zip(control.iter()) {
                    assert_eq!(initial.0, control.0);
                    assert_eq!(*initial.1, control.1.as_str());
                }
            },
            _ => unreachable!(),
        }
    }
}
