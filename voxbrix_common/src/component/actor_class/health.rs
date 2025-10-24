use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Health {
    current: u32,
    maximum: u32,
}

impl Health {
    pub fn full(maximum: u32) -> Self {
        Self {
            current: maximum,
            maximum,
        }
    }

    pub fn current(&self) -> u32 {
        self.current
    }

    pub fn ratio(&self) -> Option<f32> {
        let ratio = self.current as f32 / self.maximum as f32;

        if ratio.is_nan() {
            return None;
        }

        Some(ratio.min(1.0))
    }

    pub fn damage(&mut self, amount: u32) {
        self.current = self.current.saturating_sub(amount);
    }
}
