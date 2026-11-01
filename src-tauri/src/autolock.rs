//! Idle auto-lock. Wall-clock based so time spent asleep counts as idle.
use std::time::{Duration, SystemTime};

pub struct AutoLock {
    last: SystemTime,
}

impl AutoLock {
    pub fn new(now: SystemTime) -> Self {
        Self { last: now }
    }

    pub fn touch(&mut self, now: SystemTime) {
        self.last = now;
    }

    /// `minutes == 0` means never. A clock that moved backwards is not due.
    pub fn is_due(&self, now: SystemTime, minutes: u32) -> bool {
        minutes != 0
            && now
                .duration_since(self.last)
                .is_ok_and(|idle| idle >= Duration::from_secs(u64::from(minutes) * 60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(60);
    fn t0() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
    }

    #[test]
    fn due_after_timeout() {
        let a = AutoLock::new(t0());
        assert!(a.is_due(t0() + 15 * MIN, 15));
        assert!(!a.is_due(t0() + 14 * MIN, 15));
    }

    #[test]
    fn zero_means_never() {
        assert!(!AutoLock::new(t0()).is_due(t0() + 1000 * MIN, 0));
    }

    #[test]
    fn touch_resets() {
        let mut a = AutoLock::new(t0());
        a.touch(t0() + 10 * MIN);
        assert!(!a.is_due(t0() + 20 * MIN, 15));
        assert!(a.is_due(t0() + 25 * MIN, 15));
    }

    #[test]
    fn due_after_system_sleep() {
        let a = AutoLock::new(t0());
        assert!(a.is_due(t0() + 8 * 60 * MIN, 15));
        assert!(!a.is_due(t0() - MIN, 15));
    }
}
