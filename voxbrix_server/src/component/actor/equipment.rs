use crate::component::actor::{
    ActorComponentPack,
    ActorComponentPreparePacking,
    ComponentPackerSlot,
};
use anyhow::Error;
use nohash_hasher::{
    IntMap,
    IntSet,
};
use std::collections::VecDeque;
use voxbrix_common::{
    component::actor::equipment::Equipment,
    entity::{
        actor::Actor,
        snapshot::{
            ServerSnapshot,
            MAX_SNAPSHOT_DIFF,
        },
        update::Update,
    },
    resource::item::Item,
    LabelLibrary,
};
use voxbrix_world::{
    Initialization,
    World,
};

pub struct EquipmentActorComponent {
    // First member of tuple is target of the effect, third - source of the effect
    storage: IntMap<Actor, Equipment>,
    // Actor -> Slot changes
    changes: VecDeque<(ServerSnapshot, (Actor, usize))>,
    update: Update,
}

impl EquipmentActorComponent {
    pub fn insert(
        &mut self,
        actor: Actor,
        slot: usize,
        item: Item,
        snapshot: ServerSnapshot,
    ) -> Option<Item> {
        let equipment = self.storage.entry(actor).or_default();
        if equipment.slots[slot] != Some(item) {
            self.changes.push_back((snapshot, (actor, slot)));
        }
        equipment.slots[slot].replace(item)
    }

    pub fn remove(&mut self, actor: Actor, slot: usize, snapshot: ServerSnapshot) -> Option<Item> {
        let equipment = self.storage.get_mut(&actor)?;
        let slot_mut = &mut equipment.slots[slot];
        let item = slot_mut.take();
        if item.is_some() {
            self.changes.push_back((snapshot, (actor, slot)));
            // TODO optimize by keeping counter on Equipment?
            if equipment.slots.iter().all(|item| item.is_none()) {
                self.storage.remove(&actor);
            }
        }
        item
    }
}

impl ActorComponentPack for EquipmentActorComponent {
    fn pack(
        &self,
        full_data: bool,
        last_confirmed_snapshot: ServerSnapshot,
        _player_actor: Option<&Actor>,
        actors_full_update: &IntSet<Actor>,
        actors_partial_update: &IntSet<Actor>,
        buffer: &mut Vec<u8>,
        packer_slot: &mut ComponentPackerSlot,
    ) -> Update {
        buffer.clear();
        let packer = packer_slot.take::<(Actor, usize), Item>();

        let packer = if full_data {
            let iter = actors_full_update.iter().flat_map(|actor| {
                self.storage
                    .get(actor)
                    .into_iter()
                    .flat_map(|e| e.slots.iter().enumerate())
                    .filter_map(|(slot, opt_item)| Some(((*actor, slot), opt_item.as_ref()?)))
            });

            packer.load_full(iter).pack(buffer)
        } else {
            let full_changes_iter = actors_full_update.iter().flat_map(|actor| {
                self.storage
                    .get(actor)
                    .into_iter()
                    .flat_map(|e| e.slots.iter().enumerate())
                    .filter_map(|(slot, opt_item)| Some(((*actor, slot), opt_item.as_ref()?)))
                    .map(|(k, v)| (k, Some(v)))
            });

            let first_actual_change = self
                .changes
                .iter()
                .enumerate()
                .rev()
                .take_while(|(_, (snapshot, _))| snapshot > &last_confirmed_snapshot)
                .last();

            let partial_changes_iter = first_actual_change
                .iter()
                .flat_map(|(i, _)| self.changes.range(i ..))
                .filter(|(_, (actor, _))| actors_partial_update.contains(actor))
                .map(|&(_, (actor, slot))| {
                    (
                        (actor, slot),
                        self.storage
                            .get(&actor)
                            .and_then(|eq| eq.slots[slot].as_ref()),
                    )
                });

            let iter = full_changes_iter.chain(partial_changes_iter);

            packer.load_changes(iter).pack(buffer)
        };

        packer_slot.put(packer);

        self.update
    }
}

impl ActorComponentPreparePacking for EquipmentActorComponent {
    fn prepare_packing(&mut self, snapshot: ServerSnapshot) {
        while let Some((change_snapshot, _)) = self.changes.front() {
            if snapshot.0 - change_snapshot.0 <= MAX_SNAPSHOT_DIFF {
                break;
            }

            self.changes.pop_front();
        }
    }
}

const UPDATE: &str = "actor_equipment";

impl Initialization for EquipmentActorComponent {
    type Error = Error;

    async fn initialization(world: &World) -> Result<Self, Self::Error> {
        let update = world
            .get_resource_ref::<LabelLibrary>()
            .get::<Update>(UPDATE)
            .ok_or_else(|| anyhow::anyhow!("update with label \"{}\" is undefined", UPDATE))?;

        Ok(Self {
            storage: Default::default(),
            changes: VecDeque::new(),
            update,
        })
    }
}
