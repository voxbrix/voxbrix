use nohash_hasher::IntMap;
use voxbrix_common::{
    component::actor::equipment::Equipment,
    entity::{
        actor::Actor,
        update::Update,
    },
    messages::{
        ComponentUpdateUnpack,
        UpdatesUnpacked,
    },
    pack,
    resource::item::Item,
};

pub struct EquipmentActorComponent {
    storage: IntMap<Actor, Equipment>,
    update: Update,
}

impl EquipmentActorComponent {
    pub fn new(update: Update) -> Self {
        Self {
            storage: IntMap::default(),
            update,
        }
    }

    #[allow(dead_code)]
    pub fn get(&self, actor: &Actor) -> Option<&Equipment> {
        self.storage.get(actor)
    }

    #[allow(dead_code)]
    pub fn get_slot(&self, actor: &Actor, slot: usize) -> Option<&Item> {
        self.storage.get(actor)?.slots.get(slot)?.as_ref()
    }

    pub fn unpack<'a>(&mut self, updates: &UpdatesUnpacked<'a>) {
        if let Some((changes, _)) = updates
            .get(&self.update)
            .and_then(pack::decode_from_slice::<ComponentUpdateUnpack<(Actor, usize), Item>>)
        {
            match changes {
                ComponentUpdateUnpack::Change(changes) => {
                    for ((actor, slot), change) in changes {
                        if let Some(item) = change {
                            let equipment = self.storage.entry(actor).or_default();
                            equipment.slots[slot] = Some(item);
                        } else if let Some(equipment) = self.storage.get_mut(&actor) {
                            equipment.slots[slot] = None;
                            if equipment.slots.iter().all(|item| item.is_none()) {
                                self.storage.remove(&actor);
                            }
                        }
                    }
                },
                ComponentUpdateUnpack::Full(full) => {
                    self.storage.clear();
                    for ((actor, slot), item) in full {
                        let equipment = self.storage.entry(actor).or_default();
                        equipment.slots[slot] = Some(item);
                    }
                },
            }
        }
    }
}
