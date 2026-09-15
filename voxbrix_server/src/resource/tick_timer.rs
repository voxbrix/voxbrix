use std::time::{
    Duration,
    Instant,
};
use tokio::time::{
    self,
    Interval,
    MissedTickBehavior,
};

pub struct TickTimer {
    last: Instant,
    now: Instant,
    interval: Duration,
}

impl TickTimer {
    pub fn new(interval: Duration) -> Self {
        let now = Instant::now();

        Self {
            last: now,
            now,
            interval,
        }
    }

    pub fn async_timer(&self) -> Interval {
        let mut interval = time::interval_at(time::Instant::from_std(self.now), self.interval);
        // Prefer slowed simulation over starving other work with catch-up ticks.
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        interval
    }

    pub fn record_next(&mut self) {
        self.last = self.now;
        self.now += self.interval;
    }

    pub fn elapsed(&self) -> Duration {
        self.now.saturating_duration_since(self.last)
    }
}
