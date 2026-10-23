//! Bounded additive increase / multiplicative decrease for one download batch.
//! Lowering the target drains existing requests instead of cancelling good work.

pub(crate) struct Concurrency {
    pub limit: usize,
    ceiling: usize,
    adaptive: bool,
    bytes: u64,
    retries: usize,
    completed: usize,
    idle_windows: usize,
    cooldown: usize,
}

impl Concurrency {
    pub fn new(ceiling: usize, adaptive: bool, bytes: u64, retries: usize) -> Self {
        let ceiling = ceiling.clamp(1, 32);
        Self {
            limit: if adaptive { ceiling.min(4) } else { ceiling },
            ceiling,
            adaptive,
            bytes,
            retries,
            completed: 0,
            idle_windows: 0,
            cooldown: 0,
        }
    }

    /// Called once per second while there is queued work. Transfer progress also
    /// counts, so a batch of large files does not look stalled before completion.
    pub fn observe(&mut self, bytes: u64, retries: usize, completed: usize) {
        if !self.adaptive {
            return;
        }
        let progressing = bytes > self.bytes || completed > self.completed;
        self.idle_windows = if progressing {
            0
        } else {
            self.idle_windows + 1
        };
        if retries > self.retries || self.idle_windows >= 3 {
            self.limit = (self.limit / 2).max(1);
            self.cooldown = 2;
            self.idle_windows = 0;
        } else if self.cooldown > 0 {
            self.cooldown -= 1;
        } else if progressing {
            self.limit = (self.limit + 1).min(self.ceiling);
        }
        self.bytes = bytes;
        self.retries = retries;
        self.completed = completed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ramps_on_progress_backs_off_on_retries_and_recovers_without_exceeding_ceiling() {
        let mut state = Concurrency::new(8, true, 0, 0);
        assert_eq!(state.limit, 4);
        for sample in 1..=8 {
            state.observe(sample * 1024, 0, 0);
        }
        assert_eq!(state.limit, 8);
        state.observe(8192, 1, 0);
        assert_eq!(state.limit, 4);
        state.observe(9000, 1, 1);
        state.observe(10000, 1, 2);
        assert_eq!(state.limit, 4); // Recovery cooldown avoids oscillation.
        state.observe(11000, 1, 3);
        assert_eq!(state.limit, 5);
        for _ in 0..3 {
            state.observe(11000, 1, 3);
        }
        assert_eq!(state.limit, 2);
        for _ in 0..30 {
            state.observe(11000, 1, 3);
        }
        assert_eq!(state.limit, 1);
    }
    #[test]
    fn manual_mode_and_small_limits_remain_exact() {
        for limit in [1, 2, 8, 32] {
            let mut state = Concurrency::new(limit, false, 0, 0);
            for sample in 0..10 {
                state.observe(0, sample, 0);
            }
            assert_eq!(state.limit, limit);
        }
        let mut state = Concurrency::new(1, true, 0, 0);
        for sample in 0..10 {
            state.observe(sample as u64, sample, sample);
        }
        assert_eq!(state.limit, 1);
    }
}
