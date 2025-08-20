use crate::{
    entity::{
        action::Action,
        dispatch::Dispatch,
        snapshot::{
            ClientSnapshot,
            ServerSnapshot,
        },
        update::Update,
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
    de::DeserializeOwned,
    Deserialize,
    Serialize,
};
use std::{
    collections::VecDeque,
    marker::PhantomData,
    mem,
};

pub mod client;
pub mod server;

pub type ClientActionsPacker = EventsPacker<Action, ClientSnapshot>;
pub type DispatchesPacker = EventsPacker<Dispatch, ServerSnapshot>;

pub type ClientActionsUnpacker = EventsUnpacker<Action, ClientSnapshot>;
pub type DispatchesUnpacker = EventsUnpacker<Dispatch, ServerSnapshot>;

pub type ClientActionsPacked<'a> = EventsPacked<'a, Action, ClientSnapshot>;
pub type DispatchesPacked<'a> = EventsPacked<'a, Dispatch, ServerSnapshot>;

pub type ClientActionsUnpacked<'a> = EventsUnpacked<'a, Action, ClientSnapshot>;
pub type DispatchesUnpacked<'a> = EventsUnpacked<'a, Dispatch, ServerSnapshot>;

/// Update data container.
/// The updates supposed to be transfered in delta manner,
/// meaining that only changed updates are in the map,
/// but new instances of an Update override the old ones for the same Update.
#[derive(Serialize, Deserialize)]
pub struct UpdatesPacked<'a>(&'a [u8]);

pub struct UpdatesUnpacked<'a> {
    origin: &'a mut UpdatesUnpacker,
    updates: IntMap<Update, &'a [u8]>,
}

impl<'a> UpdatesUnpacked<'a> {
    pub fn get(&self, update: &Update) -> Option<&'a [u8]> {
        self.updates.get(update).copied()
    }
}

impl<'a> Drop for UpdatesUnpacked<'a> {
    fn drop(&mut self) {
        let mut buffer = mem::take(&mut self.updates);

        buffer.clear();

        // Safety: all references are removed in the previous step.
        let buffer = unsafe {
            mem::transmute::<IntMap<Update, &'a [u8]>, IntMap<Update, &'static [u8]>>(buffer)
        };

        self.origin.buffer = buffer;
    }
}

#[derive(Default)]
pub struct UpdatesUnpacker {
    buffer: IntMap<Update, &'static [u8]>,
}

impl UpdatesUnpacker {
    pub fn new() -> Self {
        Self {
            buffer: IntMap::default(),
        }
    }
}

impl UpdatesUnpacker {
    pub fn unpack<'a>(
        &'a mut self,
        updates: &'a UpdatesPacked<'a>,
    ) -> Result<UpdatesUnpacked<'a>, UnpackError> {
        let mut buffer = mem::take(&mut self.buffer);

        let (size, mut offset) = pack::decode_from_slice::<u64>(updates.0).ok_or(UnpackError)?;

        let size: usize = size.try_into().map_err(|_| UnpackError)?;

        buffer.reserve(size);

        for _ in 0 .. size {
            let ((key, value), new_offset) =
                pack::decode_from_slice::<(Update, &[u8])>(&updates.0[offset ..])
                    .ok_or(UnpackError)?;

            offset += new_offset;

            buffer.insert(key, value);
        }

        let unpacked = UpdatesUnpacked {
            origin: self,
            updates: buffer,
        };

        Ok(unpacked)
    }
}

pub struct UpdatesPacker {
    packed_updates: IntSet<Update>,
    updates: IntMap<Update, Vec<u8>>,
    to_be_cleared: bool,
    buffer: Vec<u8>,
}

impl UpdatesPacker {
    pub fn new() -> Self {
        Self {
            packed_updates: IntSet::default(),
            updates: IntMap::default(),
            to_be_cleared: false,
            buffer: Vec::new(),
        }
    }

    pub fn get_buffer(&mut self, update: Update) -> &mut Vec<u8> {
        if self.to_be_cleared {
            self.to_be_cleared = false;

            for update in self.packed_updates.drain() {
                self.updates.get_mut(&update).unwrap().clear();
            }
        }

        if self.updates.get(&update).is_none() {
            self.updates.insert(update, Vec::new());
        }

        let buffer = self.updates.get_mut(&update).unwrap();

        self.packed_updates.insert(update);

        buffer
    }

    pub fn pack<'a>(&'a mut self) -> UpdatesPacked<'a> {
        let extend_iter = self
            .packed_updates
            .iter()
            .map(|comp| self.updates.get_key_value(comp).unwrap());

        let updates_count = self.packed_updates.len();

        self.buffer.clear();

        pack::encode_write(&(updates_count as u64), &mut self.buffer);

        for update in extend_iter {
            pack::encode_write(&update, &mut self.buffer);
        }

        self.to_be_cleared = true;

        UpdatesPacked(self.buffer.as_slice())
    }
}

#[derive(Deserialize)]
pub enum ComponentUpdateUnpack<E, T> {
    Full(Vec<(E, T)>),
    Change(Vec<(E, Option<T>)>),
}

#[derive(Serialize)]
pub enum ComponentUpdatePack<'a, E, T> {
    Full(&'a [(E, &'a T)]),
    Change(&'a [(E, Option<&'a T>)]),
}

#[derive(Serialize, Deserialize)]
pub struct EventsPacked<'a, E, S> {
    data: &'a [u8],
    _ty: PhantomData<(E, S)>,
}

pub struct EventsUnpacked<'a, E, S> {
    origin: &'a mut EventsUnpacker<E, S>,
    data: Vec<(E, S, &'a [u8])>,
}

impl<'a, E, S> EventsUnpacked<'a, E, S> {
    pub fn data(&self) -> &[(E, S, &'a [u8])] {
        self.data.as_slice()
    }
}

impl<'a, E, S> Drop for EventsUnpacked<'a, E, S> {
    fn drop(&mut self) {
        let mut buffer = mem::take(&mut self.data);

        buffer.clear();

        // Safety: all references are removed in the previous step.
        let buffer =
            unsafe { mem::transmute::<Vec<(E, S, &'a [u8])>, Vec<(E, S, &'static [u8])>>(buffer) };

        self.origin.buffer = buffer;
    }
}

#[derive(Default)]
pub struct EventsUnpacker<E, S> {
    buffer: Vec<(E, S, &'static [u8])>,
}

impl<E, S> EventsUnpacker<E, S> {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }
}

impl<E, S> EventsUnpacker<E, S>
where
    E: DeserializeOwned,
    S: DeserializeOwned,
{
    pub fn unpack<'a>(
        &'a mut self,
        events: &'a EventsPacked<'a, E, S>,
    ) -> Result<EventsUnpacked<'a, E, S>, UnpackError> {
        let mut buffer = mem::take(&mut self.buffer);

        let (size, mut offset) = pack::decode_from_slice::<u64>(events.data).ok_or(UnpackError)?;

        let size: usize = size.try_into().map_err(|_| UnpackError)?;

        buffer.reserve(size);

        for _ in 0 .. size {
            let ((event, snapshot, data), new_offset) =
                pack::decode_from_slice::<(E, S, &[u8])>(&events.data[offset ..])
                    .ok_or(UnpackError)?;

            offset += new_offset;

            buffer.push((event, snapshot, data));
        }

        let unpacked = EventsUnpacked {
            origin: self,
            data: buffer,
        };

        Ok(unpacked)
    }
}

pub struct EventsPacker<E, S> {
    buffer: Vec<u8>,
    events: VecDeque<(S, E, usize)>,
    data: VecDeque<u8>,
}

impl<E, S> EventsPacker<E, S>
where
    E: Serialize + Copy + Ord,
    S: Serialize + Copy + Ord,
{
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            events: VecDeque::new(),
            data: VecDeque::new(),
        }
    }

    pub fn add(&mut self, event: E, snapshot: S, data: impl Serialize) {
        let size = pack::encode_write(&data, &mut self.data);

        self.events.push_back((snapshot, event, size));
    }

    fn remove_event(&mut self) {
        if let Some((_, _, size)) = self.events.pop_front() {
            self.data.drain(.. size);
        }
    }

    pub fn confirm_snapshot(&mut self, snapshot: S) {
        loop {
            match self.events.front() {
                Some((event_snapshot, _, _)) if *event_snapshot <= snapshot => {
                    self.remove_event();
                },
                _ => break,
            }
        }
    }

    pub fn pack<'a>(&'a mut self) -> EventsPacked<'a, E, S> {
        let mut data_cursor = 0;

        let data_slice: &_ = self.data.make_contiguous();

        let extend_iter = self.events.iter().copied().map(|(snapshot, event, size)| {
            let read_from = data_cursor;
            data_cursor += size;
            (event, snapshot, &data_slice[read_from .. data_cursor])
        });

        let updates_count = self.events.len();

        self.buffer.clear();

        pack::encode_write(&(updates_count as u64), &mut self.buffer);

        for element in extend_iter {
            pack::encode_write(&element, &mut self.buffer);
        }

        EventsPacked {
            data: self.buffer.as_slice(),
            _ty: PhantomData,
        }
    }
}

pub enum LoadedData {
    Changes,
    Full,
    None,
}

pub struct ComponentPacker<'a, E, T> {
    loaded_data: LoadedData,
    data_changes: Vec<(E, Option<&'a T>)>,
    data_full: Vec<(E, &'a T)>,
}

impl<E, T> ComponentPacker<'static, E, T>
where
    E: Serialize,
    T: Serialize,
{
    pub fn new() -> Self {
        Self {
            loaded_data: LoadedData::None,
            data_full: Vec::new(),
            data_changes: Vec::new(),
        }
    }

    pub fn load_changes<'a>(
        self,
        iter: impl Iterator<Item = (E, Option<&'a T>)>,
    ) -> ComponentPacker<'a, E, T> {
        let mut new = self;
        new.data_changes.extend(iter);
        new.loaded_data = LoadedData::Changes;
        new
    }

    pub fn load_full<'a>(
        self,
        iter: impl Iterator<Item = (E, &'a T)>,
    ) -> ComponentPacker<'a, E, T> {
        let mut new = self;
        new.data_full.extend(iter);
        new.loaded_data = LoadedData::Full;
        new
    }
}

impl<'a, E, T> ComponentPacker<'a, E, T>
where
    E: Serialize,
    T: Serialize,
{
    pub fn pack(mut self, buffer: &mut Vec<u8>) -> ComponentPacker<'static, E, T> {
        match self.loaded_data {
            LoadedData::Changes => {
                let msg = ComponentUpdatePack::Change(&self.data_changes);
                pack::encode_into(&msg, buffer);
            },
            LoadedData::Full => {
                let msg = ComponentUpdatePack::Full(&self.data_full);
                pack::encode_into(&msg, buffer);
            },
            LoadedData::None => {
                panic!("no changes loaded");
            },
        }

        self.data_changes.clear();
        self.data_full.clear();
        self.loaded_data = LoadedData::None;

        // Safety: the `self.data` is `Vec` that contains references with lifetime `'a`.
        // It is the only field of the struct that utilizes the `'a` lifetime and since we
        // empty the `Vec` with `clear()` on the previous step, this `unsafe` should be sound.
        unsafe { mem::transmute::<ComponentPacker<'a, E, T>, ComponentPacker<'static, E, T>>(self) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack;

    #[test]
    fn test_actor_update_serde() {
        let initial = ComponentUpdatePack::Full(&[("Key3", &"Value3")]);
        let mut buffer = Vec::new();
        pack::encode_write(&initial, &mut buffer);
        let (control, _) =
            pack::decode_from_slice::<ComponentUpdateUnpack<String, String>>(&buffer).unwrap();

        match (initial, control) {
            (ComponentUpdatePack::Full(initial), ComponentUpdateUnpack::Full(control)) => {
                assert_eq!(initial[0].0, control[0].0);
                assert_eq!(*initial[0].1, control[0].1.as_str());
            },
            _ => unreachable!(),
        }

        let initial = ComponentUpdatePack::Full(&[
            ("Key7", &"Value7"),
            ("Key1", &"Value1"),
            ("Key13", &"Value13"),
        ]);
        let mut buffer = Vec::new();
        pack::encode_write(&initial, &mut buffer);
        let (control, _) =
            pack::decode_from_slice::<ComponentUpdateUnpack<String, String>>(&buffer).unwrap();

        match (initial, control) {
            (ComponentUpdatePack::Full(initial), ComponentUpdateUnpack::Full(control)) => {
                for (initial, control) in initial.iter().zip(control.iter()) {
                    assert_eq!(initial.0, control.0);
                    assert_eq!(*initial.1, control.1.as_str());
                }
            },
            _ => unreachable!(),
        }
    }
}
