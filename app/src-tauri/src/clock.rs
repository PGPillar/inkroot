//! Time abstraction so waits and timestamps are testable without real delays.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub trait Clock: Send + Sync {
    /// Monotonic time since the clock started.
    fn now(&self) -> Duration;
    fn sleep(&self, d: Duration);
    fn wall(&self) -> chrono::DateTime<chrono::Local>;
}

pub struct SystemClock {
    start: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }
    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }
    fn wall(&self) -> chrono::DateTime<chrono::Local> {
        chrono::Local::now()
    }
}

/// Virtual clock: `sleep` advances time instantly. Clones share the same timeline.
#[derive(Clone)]
pub struct FakeClock {
    start_wall: chrono::DateTime<chrono::Local>,
    elapsed: Arc<Mutex<Duration>>,
}

impl FakeClock {
    pub fn new(start_wall: chrono::DateTime<chrono::Local>) -> Self {
        Self {
            start_wall,
            elapsed: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        *self.elapsed.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn sleep(&self, d: Duration) {
        *self.elapsed.lock().unwrap_or_else(|e| e.into_inner()) += d;
    }
    fn wall(&self) -> chrono::DateTime<chrono::Local> {
        self.start_wall + chrono::Duration::from_std(self.now()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_sleep_advances() {
        let start = chrono::Local::now();
        let c = FakeClock::new(start);
        let real = std::time::Instant::now();
        c.sleep(Duration::from_secs(60));
        assert_eq!(c.now(), Duration::from_secs(60));
        assert_eq!(c.wall(), start + chrono::Duration::seconds(60));
        assert!(real.elapsed() < Duration::from_secs(1));
    }
}
