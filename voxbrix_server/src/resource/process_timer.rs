use std::time::{
    Duration,
    Instant,
};

pub struct ProcessTimer {
    last: Instant,
    elapsed: Duration,
}

impl ProcessTimer {
    pub fn new() -> Self {
        Self {
            last: Instant::now(),
            elapsed: Duration::ZERO,
        }
    }

    pub fn record_next(&mut self) {
        let now = Instant::now();

        let elapsed = now.saturating_duration_since(self.last);

        self.last = now;
        self.elapsed = elapsed;
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }
}
