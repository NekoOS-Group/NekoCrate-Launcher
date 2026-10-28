//! Detect inactivity, not low bandwidth or slow disk writes. A timed-out attempt
//! keeps its partial object; recovery reconnects and uses the existing Range path.
use std::time::Duration;

#[derive(Clone, Copy)]
pub(crate) struct StallPolicy {
    pub idle: Duration,
    pub headers: Duration,
    pub ceiling: Duration,
}
impl Default for StallPolicy {
    fn default() -> Self {
        Self {
            idle: Duration::from_secs(8),
            headers: Duration::from_secs(15),
            ceiling: Duration::from_secs(45),
        }
    }
}
impl StallPolicy {
    pub fn wait(self, cadence: Duration, stalls: u32, headers: bool) -> Duration {
        let floor = if headers { self.headers } else { self.idle };
        floor
            .max(cadence.saturating_mul(8))
            .saturating_mul(1 << stalls.min(3))
            .min(self.ceiling)
    }
}

#[derive(Debug)]
pub(crate) struct Stalled(pub Duration);
impl std::fmt::Display for Stalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "下载连接连续 {:.1} 秒未收到数据", self.0.as_secs_f64())
    }
}
impl std::error::Error for Stalled {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stall_deadlines_allow_sparse_data_and_back_off_after_reconnection() {
        let policy = StallPolicy::default();
        assert_eq!(
            policy.wait(Duration::ZERO, 0, false),
            Duration::from_secs(8)
        );
        assert_eq!(
            policy.wait(Duration::ZERO, 0, true),
            Duration::from_secs(15)
        );
        assert_eq!(
            policy.wait(Duration::from_secs(2), 0, false),
            Duration::from_secs(16)
        );
        assert_eq!(
            policy.wait(Duration::ZERO, 1, false),
            Duration::from_secs(16)
        );
        assert_eq!(
            policy.wait(Duration::from_secs(2), 5, false),
            Duration::from_secs(45)
        );
    }
}
