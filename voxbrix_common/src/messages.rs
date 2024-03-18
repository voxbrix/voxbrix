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
use bincode::{
    de::Deserializer,
    Options,
    Serializer,
};
use nohash_hasher::IntMap;
use serde::{
    de::{
        Deserializer as _,
        SeqAccess,
        Visitor,
    },
    ser::{
        SerializeSeq,
        Serializer as _,
    },
    Deserialize,
    Serialize,
};
use std::{
    collections::VecDeque,
    fmt,
    io::Write,
    mem,
};

pub mod client;
pub mod server;

/// State container.
/// The components supposed to be transfered in delta manner,
/// meaining that only changed components are in the map.
#[derive(Serialize, Deserialize)]
pub struct StatePacked<'a>(#[serde(borrow)] &'a [u8]);

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

impl<'a> Visitor<'a> for &mut StateUnpacked<'a> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "an array of integers")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'a>,
    {
        // Visit each element in the inner array and push it onto
        // the existing vector.
        while let Some((component, data)) = seq.next_element::<(StateComponent, &'a [u8])>()? {
            self.components.insert(component, data);
        }
        Ok(())
    }
}

impl StateUnpacker {
    pub fn unpack_state<'a>(
        &'a mut self,
        state: StatePacked<'a>,
    ) -> Result<StateUnpacked<'a>, UnpackError> {
        let buffer = mem::take(&mut self.buffer);

        let mut unpacked = StateUnpacked {
            origin: self,
            components: buffer,
        };

        Deserializer::from_slice(state.0, pack::packer())
            .deserialize_seq(&mut unpacked)
            .map_err(|_| UnpackError)?;

        Ok(unpacked)
    }
}

pub struct StatePacker {
    components: IntMap<StateComponent, (bool, Vec<u8>)>,
    to_be_cleared: bool,
    buffer: Vec<u8>,
}

impl StatePacker {
    pub fn new() -> Self {
        Self {
            components: IntMap::default(),
            to_be_cleared: false,
            buffer: Vec::new(),
        }
    }

    pub fn get_component_buffer(&mut self, component: StateComponent) -> &mut Vec<u8> {
        if self.to_be_cleared {
            self.to_be_cleared = false;

            for (_, (is_packed, buffer)) in self.components.iter_mut() {
                *is_packed = false;
                buffer.clear();
            }
        }

        if self.components.get(&component).is_none() {
            self.components.insert(component, (false, Vec::new()));
        }

        let (is_packed, buffer) = self.components.get_mut(&component).unwrap();

        *is_packed = true;

        buffer
    }

    pub fn pack_state<'a>(&'a mut self) -> StatePacked<'a> {
        let extend_iter = self
            .components
            .iter()
            .filter_map(|(component, (is_packed, buffer))| {
                is_packed.then_some((*component, buffer.as_slice()))
            });

        let components_count = extend_iter.clone().count();

        self.buffer.clear();

        let mut serializer = Serializer::new(&mut self.buffer, pack::packer());

        let mut seq = serializer
            .serialize_seq(Some(components_count))
            .expect("serialization should not fail");

        for element in extend_iter {
            seq.serialize_element(&element)
                .expect("serialization should not fail");
        }

        seq.end().expect("serialization should not fail");

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

struct WriteCount<T> {
    written: usize,
    write_to: T,
}

impl<T> WriteCount<T>
where
    T: Write,
{
    fn new(write_to: T) -> Self {
        Self {
            written: 0,
            write_to,
        }
    }

    fn written(&self) -> usize {
        self.written
    }
}

impl<T> Write for WriteCount<T>
where
    T: Write,
{
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = self.write_to.write(buf)?;
        self.written += written;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.write_to.flush()
    }
}

#[derive(Serialize, Deserialize)]
pub struct ActionsPacked<'a>(#[serde(borrow)] &'a [u8]);

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

pub struct ActionsUnpacker {
    buffer: Vec<(Action, Snapshot, &'static [u8])>,
}

impl ActionsUnpacker {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }
}

impl<'a> Visitor<'a> for &mut ActionsUnpacked<'a> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "an array of integers")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'a>,
    {
        // Visit each element in the inner array and push it onto
        // the existing vector.
        while let Some(next) = seq.next_element::<(Action, Snapshot, &'a [u8])>()? {
            self.data.push(next);
        }

        Ok(())
    }
}

impl ActionsUnpacker {
    pub fn unpack_actions<'a>(
        &'a mut self,
        actions: ActionsPacked<'a>,
    ) -> Result<ActionsUnpacked<'a>, UnpackError> {
        let buffer = mem::take(&mut self.buffer);

        let mut unpacked = ActionsUnpacked {
            origin: self,
            data: buffer,
        };

        Deserializer::from_slice(actions.0, pack::packer())
            .deserialize_seq(&mut unpacked)
            .map_err(|_| UnpackError)?;

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
        let mut write_count = WriteCount::new(&mut self.data);

        pack::packer()
            .serialize_into(&mut write_count, &data)
            .unwrap();

        let written = write_count.written();

        self.actions.push_back((snapshot, action, written));
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

        let mut serializer = Serializer::new(&mut self.buffer, pack::packer());

        let mut seq = serializer
            .serialize_seq(Some(components_count))
            .expect("serialization should not fail");

        for element in extend_iter {
            seq.serialize_element(&element)
                .expect("serialization should not fail");
        }

        seq.end().expect("serialization should not fail");

        ActionsPacked(self.buffer.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::Options;

    fn sd_opts() -> impl Options {
        bincode::DefaultOptions::default()
    }

    #[test]
    fn test_actor_state_serde() {
        let initial = ActorStatePack::Full(&[(Actor(3), &"Actor3")]);
        let buffer = sd_opts().serialize(&initial).unwrap();
        let control = sd_opts()
            .deserialize::<ActorStateUnpack<String>>(&buffer)
            .unwrap();

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
        let buffer = sd_opts().serialize(&initial).unwrap();
        let control = sd_opts()
            .deserialize::<ActorStateUnpack<String>>(&buffer)
            .unwrap();

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

// TODO try implement as scripts
use crate::entity::{
    block::Block,
    chunk::Chunk,
};

#[derive(Serialize, Deserialize)]
pub struct PlaceBlockAction {
    chunk: Chunk,
    block: Block,
}

#[derive(Serialize, Deserialize)]
pub struct RemoveBlockAction {
    chunk: Chunk,
    block: Block,
}
