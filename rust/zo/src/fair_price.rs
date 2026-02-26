//! Offset-median fair price calculator.
//!
//! Computes `fair_price = reference_mid + median(local_mid - reference_mid)`
//! using per-second offset samples over a configurable time window.
//! A circular buffer avoids allocations after warmup.

/// Maximum samples retained in the circular buffer (>5 min at 1/s).
const MAX_SAMPLES: usize = 500;

/// Configuration for the fair price calculator.
#[derive(Debug, Clone)]
pub struct FairPriceConfig {
    /// Time window for valid samples in milliseconds (e.g. 300_000 for 5 min).
    pub window_ms: u64,
    /// Minimum samples required before producing a fair price.
    pub min_samples: usize,
}

/// Snapshot of the calculator's current state (for debugging / display).
#[derive(Debug, Clone)]
pub struct FairPriceState {
    /// Raw median offset (ignores `min_samples`), or `None` if no samples.
    pub offset: Option<f64>,
    /// Number of valid (non-expired) samples.
    pub samples: usize,
}

/// A single offset sample: `local_mid - reference_mid` at a given second.
#[derive(Clone, Copy)]
struct OffsetSample {
    offset: f64,
    second: u64,
}

/// Fair price calculator using a circular buffer of per-second offset samples.
///
/// # Algorithm
///
/// Each second, the caller feeds the local exchange mid-price and a reference
/// exchange mid-price. The calculator stores `offset = local - reference` in a
/// circular buffer keyed by Unix second (deduplicating within the same second).
///
/// To produce a fair price the calculator:
/// 1. Collects all samples within `window_ms` of the current time.
/// 2. Computes the median of those offsets using `select_nth_unstable` (O(n)).
/// 3. Returns `reference_mid + median_offset`.
pub struct FairPriceCalculator {
    config: FairPriceConfig,
    /// Pre-allocated ring buffer.
    samples: Vec<OffsetSample>,
    /// Next write position (wraps around at `MAX_SAMPLES`).
    head: usize,
    /// Number of samples written so far (capped at `MAX_SAMPLES`).
    count: usize,
    /// Last recorded Unix second (for dedup).
    last_second: u64,
}

impl FairPriceCalculator {
    /// Create a new calculator with the given configuration.
    pub fn new(config: FairPriceConfig) -> Self {
        Self {
            config,
            samples: Vec::with_capacity(MAX_SAMPLES),
            head: 0,
            count: 0,
            last_second: 0,
        }
    }

    /// Record a price sample. Only one sample per second is retained.
    ///
    /// # Arguments
    ///
    /// * `local_mid` - Mid-price from the local exchange (e.g. 01).
    /// * `reference_mid` - Mid-price from the reference exchange (e.g. Binance).
    /// * `now_ms` - Current wall-clock time in epoch milliseconds.
    pub fn add_sample(&mut self, local_mid: f64, reference_mid: f64, now_ms: u64) {
        let current_second = now_ms / 1000;

        // Deduplicate: at most one sample per second.
        if current_second <= self.last_second {
            return;
        }
        self.last_second = current_second;

        let sample = OffsetSample {
            offset: local_mid - reference_mid,
            second: current_second,
        };

        // Write into the circular buffer.
        if self.samples.len() < MAX_SAMPLES {
            self.samples.push(sample);
        } else {
            self.samples[self.head] = sample;
        }
        self.head = (self.head + 1) % MAX_SAMPLES;
        if self.count < MAX_SAMPLES {
            self.count += 1;
        }
    }

    /// Get the fair price: `reference_mid + median(offsets)`.
    ///
    /// Returns `None` if fewer than `min_samples` valid samples exist.
    pub fn get_fair_price(&self, reference_mid: f64, now_ms: u64) -> Option<f64> {
        let offset = self.get_median_offset(now_ms)?;
        Some(reference_mid + offset)
    }

    /// Get the median offset, respecting the `min_samples` threshold.
    pub fn get_median_offset(&self, now_ms: u64) -> Option<f64> {
        let mut offsets = self.collect_valid_offsets(now_ms);
        if offsets.len() < self.config.min_samples {
            return None;
        }
        Some(compute_median(&mut offsets))
    }

    /// Get the raw median offset (ignores `min_samples`; useful during warmup display).
    pub fn get_raw_median_offset(&self, now_ms: u64) -> Option<f64> {
        let mut offsets = self.collect_valid_offsets(now_ms);
        if offsets.is_empty() {
            return None;
        }
        Some(compute_median(&mut offsets))
    }

    /// Number of valid (non-expired) samples.
    pub fn get_sample_count(&self, now_ms: u64) -> usize {
        let cutoff = cutoff_second(now_ms, self.config.window_ms);
        self.samples[..self.count]
            .iter()
            .filter(|s| s.second > cutoff)
            .count()
    }

    /// Snapshot of the current state for debugging / display.
    pub fn get_state(&self, now_ms: u64) -> FairPriceState {
        FairPriceState {
            offset: self.get_raw_median_offset(now_ms),
            samples: self.get_sample_count(now_ms),
        }
    }

    /// Collect offsets from samples within the time window.
    fn collect_valid_offsets(&self, now_ms: u64) -> Vec<f64> {
        let cutoff = cutoff_second(now_ms, self.config.window_ms);
        self.samples[..self.count]
            .iter()
            .filter(|s| s.second > cutoff)
            .map(|s| s.offset)
            .collect()
    }
}

/// The cutoff second: samples at or before this second are expired.
fn cutoff_second(now_ms: u64, window_ms: u64) -> u64 {
    now_ms.saturating_sub(window_ms) / 1000
}

/// O(n) median via `select_nth_unstable` (introselect).
///
/// For even-length slices, returns the average of the two middle elements.
/// The input slice is partially reordered (acceptable since we own it).
fn compute_median(values: &mut [f64]) -> f64 {
    let n = values.len();
    debug_assert!(n > 0);

    let mid = n / 2;
    // `select_nth_unstable_by` partitions so that values[mid] is the (mid+1)-th
    // smallest element, with everything before it <= and everything after >=.
    values.select_nth_unstable_by(mid, |a, b| a.partial_cmp(b).unwrap());

    if n % 2 == 1 {
        values[mid]
    } else {
        // For even n we also need the (mid-1)-th element, which is the max of
        // the left partition (indices 0..mid).
        let left_max = values[..mid]
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        (left_max + values[mid]) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(min_samples: usize) -> FairPriceConfig {
        FairPriceConfig {
            window_ms: 300_000, // 5 min
            min_samples,
        }
    }

    #[test]
    fn test_one_sample_per_second_dedup() {
        let mut calc = FairPriceCalculator::new(cfg(1));
        // Two samples in the same second — only the first is kept.
        calc.add_sample(100.0, 99.0, 5_000);
        calc.add_sample(200.0, 99.0, 5_500); // same second (5)
        assert_eq!(calc.get_sample_count(6_000), 1);
    }

    #[test]
    fn test_returns_none_below_min_samples() {
        let mut calc = FairPriceCalculator::new(cfg(3));
        calc.add_sample(100.0, 99.0, 1_000);
        calc.add_sample(101.0, 99.0, 2_000);
        // Only 2 samples, need 3.
        assert!(calc.get_fair_price(99.0, 3_000).is_none());
    }

    #[test]
    fn test_median_odd_count() {
        let mut calc = FairPriceCalculator::new(cfg(1));
        // offsets: 1.0, 2.0, 3.0 → median = 2.0
        calc.add_sample(100.0, 99.0, 1_000); // offset 1.0
        calc.add_sample(101.0, 99.0, 2_000); // offset 2.0
        calc.add_sample(102.0, 99.0, 3_000); // offset 3.0
        let median = calc.get_median_offset(4_000).unwrap();
        assert!((median - 2.0).abs() < 1e-12);
    }

    #[test]
    fn test_median_even_count() {
        let mut calc = FairPriceCalculator::new(cfg(1));
        // offsets: 1.0, 2.0, 3.0, 4.0 → median = (2+3)/2 = 2.5
        calc.add_sample(100.0, 99.0, 1_000); // 1.0
        calc.add_sample(101.0, 99.0, 2_000); // 2.0
        calc.add_sample(102.0, 99.0, 3_000); // 3.0
        calc.add_sample(103.0, 99.0, 4_000); // 4.0
        let median = calc.get_median_offset(5_000).unwrap();
        assert!((median - 2.5).abs() < 1e-12);
    }

    #[test]
    fn test_window_expiry() {
        let mut calc = FairPriceCalculator::new(FairPriceConfig {
            window_ms: 5_000, // 5 s window
            min_samples: 1,
        });
        calc.add_sample(110.0, 100.0, 1_000); // offset 10, second 1
        calc.add_sample(120.0, 100.0, 2_000); // offset 20, second 2
                                              // At now=8_000, cutoff_second = (8000-5000)/1000 = 3 → second 1 and 2 are <=3
                                              // So both samples should still be valid (second > 3 is false for both).
                                              // Actually: cutoff = 3, sample seconds are 1 and 2, both <= 3, so EXPIRED.
        assert_eq!(calc.get_sample_count(8_000), 0);

        // At now=6_000, cutoff = 1, second 2 > 1 → 1 valid
        assert_eq!(calc.get_sample_count(6_000), 1);
        let median = calc.get_median_offset(6_000).unwrap();
        assert!((median - 20.0).abs() < 1e-12);
    }

    #[test]
    fn test_fair_price_equals_reference_plus_offset() {
        let mut calc = FairPriceCalculator::new(cfg(1));
        // offset = 105.0 - 100.0 = 5.0
        calc.add_sample(105.0, 100.0, 1_000);
        let fair = calc.get_fair_price(100.0, 2_000).unwrap();
        // fair = 100.0 + 5.0 = 105.0
        assert!((fair - 105.0).abs() < 1e-12);
    }

    #[test]
    fn test_circular_buffer_wraparound() {
        let mut calc = FairPriceCalculator::new(FairPriceConfig {
            window_ms: 1_000_000, // large window so nothing expires
            min_samples: 1,
        });
        // Write MAX_SAMPLES + 100 samples to force wraparound.
        for i in 0..(MAX_SAMPLES + 100) {
            let t = ((i + 1) * 1000) as u64;
            calc.add_sample(100.0 + i as f64, 100.0, t);
        }
        // count should be capped at MAX_SAMPLES
        assert_eq!(calc.count, MAX_SAMPLES);
        assert_eq!(calc.samples.len(), MAX_SAMPLES);

        // The oldest surviving offset is (MAX_SAMPLES+100) - MAX_SAMPLES = 100
        // (i.e. offsets 100..599). Median of 100..599 = (349+350)/2 = 349.5
        let now = ((MAX_SAMPLES + 101) * 1000) as u64;
        let median = calc.get_median_offset(now).unwrap();
        assert!((median - 349.5).abs() < 1e-12);
    }

    #[test]
    fn test_raw_median_ignores_min_samples() {
        let mut calc = FairPriceCalculator::new(cfg(100)); // unreachably high min
        calc.add_sample(105.0, 100.0, 1_000); // offset 5.0
                                              // get_median_offset returns None (only 1 sample, need 100)
        assert!(calc.get_median_offset(2_000).is_none());
        // get_raw_median_offset returns the value regardless
        let raw = calc.get_raw_median_offset(2_000).unwrap();
        assert!((raw - 5.0).abs() < 1e-12);
    }

    #[test]
    fn test_get_state() {
        let mut calc = FairPriceCalculator::new(cfg(1));
        calc.add_sample(101.0, 100.0, 1_000);
        calc.add_sample(103.0, 100.0, 2_000);
        let state = calc.get_state(3_000);
        assert_eq!(state.samples, 2);
        // offsets: 1.0, 3.0 → median = 2.0
        assert!((state.offset.unwrap() - 2.0).abs() < 1e-12);
    }
}
