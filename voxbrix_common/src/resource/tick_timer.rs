use std::time::{
    Duration,
    Instant,
};

pub struct TickTimer {
    last: Instant,
    elapsed: Duration,
}

impl TickTimer {
    pub fn start() -> Self {
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

    pub fn now(&self) -> Instant {
        self.last
    }
}
