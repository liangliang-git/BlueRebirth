use std::time::{SystemTime, UNIX_EPOCH};

pub type UnixSeconds = u32;

pub trait Clock: Send + Sync {
    fn now(&self) -> UnixSeconds;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UnixSeconds {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().min(u64::from(u32::MAX)) as u32)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub UnixSeconds);

impl Clock for FixedClock {
    fn now(&self) -> UnixSeconds {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{Clock, FixedClock};

    #[test]
    fn fixed_clock_makes_reset_rules_deterministic() {
        assert_eq!(FixedClock(1_700_000_000).now(), 1_700_000_000);
    }
}
