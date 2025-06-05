use crate::window::Frame;
use egui::Context;

/// Accumulator for all interface elements to be rendered.
pub struct Interface {
    context: Option<Context>,
}

impl Interface {
    pub fn new() -> Self {
        Self { context: None }
    }

    /// Call this before adding elements.
    pub fn initialize(&mut self, frame: &mut Frame) {
        self.context = Some(frame.ui_renderer.context().clone());

        self.context
            .as_ref()
            .unwrap()
            .begin_pass(frame.take_ui_input());
    }

    pub fn add_element(&self, element: impl FnOnce(&Context)) {
        element(
            self.context
                .as_ref()
                .expect("must initialize interface before adding interface element"),
        );
    }

    pub fn finalize(&mut self) -> Context {
        self.context.take().expect("interface is not initialized")
    }
}
