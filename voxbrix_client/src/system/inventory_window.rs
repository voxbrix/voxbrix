use crate::resource::{
    interface::Interface,
    interface_state::InterfaceState,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct InventoryWindowSystem;

impl System for InventoryWindowSystem {
    type Data<'a> = InventoryWindowSystemData<'a>;
}

#[derive(SystemData)]
pub struct InventoryWindowSystemData<'a> {
    interface: &'a Interface,
    interface_state: &'a mut InterfaceState,
}

impl InventoryWindowSystemData<'_> {
    pub fn run(self) {
        self.interface.add_element(|ctx| {
            egui::Window::new("Inventory")
                .open(&mut self.interface_state.inventory_open)
                .show(ctx, |ui| {
                    ui.label("Hello World!");
                });
        });
    }
}
