//! Probe download throughput, keeping extra slots only when they improve it.
//! Reductions drain existing requests; they never cancel useful transfers.

use std::time::Duration;

pub(crate) const MAX_CONCURRENCY: usize = 256;
const WINDOW: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
struct Rate {
    bytes: f64,
    files: f64,
}
impl Rate {
    // Cache-only work uses files/s; cache hits cannot inflate network throughput.
    // A switch between local and network work needs a fresh baseline.
    fn relative_to(self, previous: Self) -> Option<f64> {
        if self.bytes > 0.0 && previous.bytes > 0.0 {
            Some(self.bytes / previous.bytes)
        } else if self.bytes == 0.0 && previous.bytes == 0.0 && previous.files > 0.0 {
            Some(self.files / previous.files)
        } else {
            None
        }
    }
    fn smooth(self, previous: Self) -> Self {
        Self {
            bytes: previous.bytes * 0.75 + self.bytes * 0.25,
            files: previous.files * 0.75 + self.files * 0.25,
        }
    }
}
#[derive(Default)]
struct Sample {
    elapsed: Duration,
    bytes: u64,
    files: usize,
}
struct Probe {
    previous_limit: usize,
    baseline: Rate,
}

pub(crate) struct Concurrency {
    pub limit: usize,
    ceiling: usize,
    adaptive: bool,
    bytes: u64,
    retries: usize,
    completed: usize,
    idle: Duration,
    cooldown: Duration,
    sample: Sample,
    steady: Option<Rate>,
    probe: Option<Probe>,
    plateaus: u32,
    slow_windows: usize,
}

impl Concurrency {
    pub fn new(ceiling: usize, adaptive: bool, bytes: u64, retries: usize) -> Self {
        let ceiling = ceiling.clamp(1, MAX_CONCURRENCY);
        Self {
            limit: if adaptive { ceiling.min(4) } else { ceiling },
            ceiling,
            adaptive,
            bytes,
            retries,
            completed: 0,
            idle: Duration::ZERO,
            cooldown: Duration::ZERO,
            sample: Sample::default(),
            steady: None,
            probe: None,
            plateaus: 0,
            slow_windows: 0,
        }
    }

    fn back_off(&mut self, limit: usize) {
        self.limit = limit.max(1);
        self.cooldown = Duration::from_secs(4);
        self.sample = Sample::default();
        self.steady = None;
        self.probe = None;
        self.slow_windows = 0;
    }

    fn probe_more(&mut self, baseline: Rate) {
        if self.limit == self.ceiling {
            return;
        }
        // Explore quickly while capacity helps; use small probes near a plateau.
        let step = if self.plateaus == 0 {
            (self.limit / 2).max(2)
        } else {
            (self.limit / 8).max(1)
        };
        self.probe = Some(Probe {
            previous_limit: self.limit,
            baseline,
        });
        self.limit = (self.limit + step).min(self.ceiling);
    }

    /// Called while queued work remains, using actual elapsed time rather than
    /// assuming timer ticks arrived on schedule. No wall-clock timestamps.
    pub fn observe(
        &mut self,
        bytes: u64,
        retries: usize,
        completed: usize,
        elapsed: Duration,
        in_flight: usize,
    ) {
        if !self.adaptive || elapsed.is_zero() {
            return;
        }
        let received = bytes.saturating_sub(self.bytes);
        let finished = completed.saturating_sub(self.completed);
        let retried = retries > self.retries;
        self.bytes = bytes;
        self.retries = retries;
        self.completed = completed;
        self.idle = if received > 0 || finished > 0 {
            Duration::ZERO
        } else {
            self.idle + elapsed
        };
        self.cooldown = self.cooldown.saturating_sub(elapsed);
        if retried || self.idle >= Duration::from_secs(3) {
            // Do not use an unsuccessful high probe as the safe baseline.
            let stable = self.probe.as_ref().map_or(self.limit, |p| p.previous_limit);
            self.back_off(stable / 2);
            self.idle = Duration::ZERO;
            return;
        }
        if in_flight != self.limit {
            // In-flight requests are still draining after a reduction (or the
            // queue is underfilled): their throughput cannot evaluate this limit.
            self.sample = Sample::default();
            return;
        }
        self.sample.elapsed += elapsed;
        self.sample.bytes += received;
        self.sample.files += finished;
        if self.sample.elapsed < WINDOW {
            return;
        }
        let sample = std::mem::take(&mut self.sample);
        let rate = Rate {
            bytes: sample.bytes as f64 / sample.elapsed.as_secs_f64(),
            files: sample.files as f64 / sample.elapsed.as_secs_f64(),
        };
        if rate.bytes == 0.0 && rate.files == 0.0 {
            return;
        }
        if let Some(probe) = self.probe.take() {
            match rate.relative_to(probe.baseline) {
                Some(gain) if gain >= 1.10 => {
                    self.plateaus = 0;
                    self.steady = Some(rate);
                    self.probe_more(rate);
                }
                gain => {
                    self.limit = probe.previous_limit;
                    self.steady = None;
                    if gain.is_some() {
                        self.cooldown = Duration::from_secs(6 * (1 << self.plateaus.min(3)));
                        self.plateaus = (self.plateaus + 1).min(3);
                    } else {
                        self.cooldown = WINDOW;
                    }
                }
            }
            return;
        }
        if let Some(previous) = self.steady {
            if let Some(change) = rate.relative_to(previous) {
                if change < 0.70 {
                    self.slow_windows += 1;
                    if self.slow_windows >= 2 {
                        self.back_off(self.limit * 3 / 4);
                    }
                    return;
                }
                self.slow_windows = 0;
                self.steady = Some(rate.smooth(previous));
            } else {
                self.slow_windows = 0;
                self.steady = Some(rate);
            }
        } else {
            self.steady = Some(rate);
        }
        if self.cooldown.is_zero() {
            self.probe_more(self.steady.unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(state: &mut Concurrency, bytes: u64, files: usize, retries: usize, seconds: u64) {
        state.observe(
            state.bytes + bytes,
            state.retries + retries,
            state.completed + files,
            Duration::from_secs(seconds),
            state.limit,
        );
    }

    #[test]
    fn manual_limits_remain_exact_and_out_of_range_values_are_bounded() {
        for limit in [1, 2, 8, 32, 128, 256] {
            let mut state = Concurrency::new(limit, false, 0, 0);
            for _ in 0..10 {
                sample(&mut state, 0, 0, 1, 1);
            }
            assert_eq!(state.limit, limit);
        }
        assert_eq!(Concurrency::new(0, false, 0, 0).limit, 1);
        assert_eq!(Concurrency::new(257, false, 0, 0).limit, 256);
        let mut state = Concurrency::new(1, true, 0, 0);
        for _ in 0..10 {
            sample(&mut state, 1024, 1, 1, 1);
        }
        assert_eq!(state.limit, 1);
    }

    #[test]
    fn sustained_throughput_gains_ramp_quickly_but_never_exceed_256() {
        let mut state = Concurrency::new(MAX_CONCURRENCY, true, 0, 0);
        assert_eq!(state.limit, 4);
        for _ in 0..16 {
            let bytes = state.limit as u64 * 1024 * 1024;
            sample(&mut state, bytes, 0, 0, 2);
            assert!(state.limit <= 256);
        }
        assert_eq!(state.limit, 256);
    }

    #[test]
    fn flat_bandwidth_rolls_back_and_spaces_out_small_probes() {
        let mut state = Concurrency::new(256, true, 0, 0);
        sample(&mut state, 1_000_000, 0, 0, 2);
        assert_eq!(state.limit, 6);
        // Even many cache hits do not make a flat network probe look successful.
        sample(&mut state, 1_000_000, 1000, 0, 2);
        assert_eq!(state.limit, 4);
        let mut probes = 0;
        for _ in 0..120 {
            let before = state.limit;
            sample(&mut state, 1_000_000, 1000, 0, 2);
            if state.limit > before {
                probes += 1;
            }
            assert!((4..=5).contains(&state.limit));
        }
        assert!(
            probes < 12,
            "must not keep probing saturated bandwidth: {probes}"
        );
    }

    #[test]
    fn retries_and_stalls_back_off_without_spending_time_at_failed_probe_limits() {
        let mut state = Concurrency::new(256, true, 0, 0);
        sample(&mut state, 1000, 0, 0, 2);
        assert_eq!(state.limit, 6);
        sample(&mut state, 0, 0, 1, 1);
        assert_eq!(state.limit, 2);
        state.observe(
            state.bytes + 3000,
            state.retries,
            state.completed + 3,
            Duration::from_secs(2),
            6,
        );
        assert_eq!(
            state.limit, 2,
            "draining old requests must not trigger growth"
        );
        sample(&mut state, 0, 0, 0, 3);
        assert_eq!(state.limit, 1);
        sample(&mut state, 2000, 1, 0, 2);
        assert_eq!(state.limit, 1, "honor the recovery cooldown");
    }

    #[test]
    fn two_slow_windows_reduce_load_but_one_noisy_window_does_not() {
        let mut state = Concurrency::new(4, true, 0, 0);
        sample(&mut state, 1000, 0, 0, 2);
        sample(&mut state, 400, 0, 0, 2);
        assert_eq!(state.limit, 4);
        sample(&mut state, 1000, 0, 0, 2);
        assert_eq!(state.limit, 4);
        sample(&mut state, 400, 0, 0, 2);
        sample(&mut state, 400, 0, 0, 2);
        assert_eq!(state.limit, 3);
    }

    #[test]
    fn delayed_ticks_use_elapsed_time_and_cache_only_work_uses_completion_rate() {
        let mut state = Concurrency::new(256, true, 0, 0);
        sample(&mut state, 2000, 0, 0, 2);
        assert_eq!(state.limit, 6);
        sample(&mut state, 4000, 0, 0, 4);
        assert_eq!(
            state.limit, 4,
            "same bytes/s over a delayed tick is not a gain"
        );
        let mut local = Concurrency::new(32, true, 0, 0);
        sample(&mut local, 0, 8, 0, 2);
        assert_eq!(local.limit, 6);
        sample(&mut local, 0, 12, 0, 2);
        assert_eq!(local.limit, 9);
        sample(&mut local, 2000, 0, 0, 2);
        assert_eq!(
            local.limit, 6,
            "switching from cache to network needs a fresh baseline"
        );
    }
}
