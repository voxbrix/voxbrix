use crate::resource::item::Item;

const MAX_EQUIPMENT_SLOTS: usize = 8;

#[derive(Default)]
pub struct Equipment {
    pub slots: [Option<Item>; MAX_EQUIPMENT_SLOTS],
}
