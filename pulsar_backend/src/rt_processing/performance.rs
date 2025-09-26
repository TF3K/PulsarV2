use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use quanta::{Clock, Instant as QuantaInstant};

/// Snapshot of metrics suitable for logging/telemetry (non-RT).
#[derive(Debug, Clone)]
pub struct PerformanceSnapshot {
    /// Total number of audio frames processed since monitor creation or last reset.
    pub frames_processed: u64,
    /// Total number of callback invocations.
    pub callback_count: u64,
    /// Total underruns reported.
    pub underrun_count: u64,
    /// Total overruns reported.
    pub overrun_count: u64,
    /// Minimum callback duration observed (ns).
    pub min_callback_nanos: Option<u64>,
    /// Maximum callback duration observed (ns) (peak).
    pub max_callback_nanos: Option<u64>,
    /// EMA of callback duration in nanoseconds.
    pub ema_callback_nanos: f64,
    /// Time when snapshot was taken.
    pub timestamp: Instant,
    pub expected_callback_nanos: f64,
    pub avg_load_percent: f64,
}

/// Real-time-safe performance monitor.
///
/// On the real-time path you should only call the `add_*`/`increment_*` methods and
/// use `scoped_callback()` for timing. Those methods use atomics only.
///
/// Snapshotting (via `snapshot`) reads atomics and computes a `PerformanceSnapshot`
/// on the non-real-time thread; calling `snapshot` is not real-time safe.
pub struct PerformanceMonitor {
    // high-resolution clock used on RT path (quanta)
    clock: Clock,
    // audio context
    frame_size: usize,
    sample_rate: f32,

    // Counters (atomics for RT safety)
    frames_processed: AtomicU64,
    callback_count: AtomicU64,
    underrun_count: AtomicU64,
    overrun_count: AtomicU64,

    // timing stats (atomics)
    min_callback_nanos: AtomicU64,
    max_callback_nanos: AtomicU64,
    /// EMA of callback duration stored as f64 bits in an AtomicU64
    ema_callback_bits: AtomicU64,

    /// EMA alpha used for updating exponential moving average on RT thread.
    ema_alpha: f64,

}

impl PerformanceMonitor {
    /// Create a new performance monitor.
    ///
    /// `ema_alpha` controls the responsiveness of the exponential moving average in
    /// callback timing. Typical small values around 0.05..0.2 work well.
    pub fn new(frame_size: usize, sample_rate: f32, ema_alpha: f64) -> Self {
        assert!(ema_alpha > 0.0 && ema_alpha <= 1.0);
        Self {
            clock: Clock::new(),
            frame_size,
            sample_rate,
            frames_processed: AtomicU64::new(0),
            callback_count: AtomicU64::new(0),
            underrun_count: AtomicU64::new(0),
            overrun_count: AtomicU64::new(0),
            min_callback_nanos: AtomicU64::new(u64::MAX),
            max_callback_nanos: AtomicU64::new(0),
            ema_callback_bits: AtomicU64::new(0u64),
            ema_alpha,
        }
    }

    // ---------------------------
    // RT-safe small operations
    // ---------------------------

    /// Increment frames processed by `n`.
    /// Real-time safe (single atomic add).
    #[inline(always)]
    pub fn add_frames_processed(&self, n: u64) {
        self.frames_processed.fetch_add(n, Ordering::Relaxed);
    }

    /// Increment callback invocation count by 1.
    /// Real-time safe.
    #[inline(always)]
    pub fn increment_callback_count(&self) {
        self.callback_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment underrun count by 1 (report underrun).
    /// Real-time safe.
    #[inline(always)]
    pub fn increment_underrun_count(&self) {
        self.underrun_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment overrun count by 1 (report overrun).
    /// Real-time safe.
    #[inline(always)]
    pub fn increment_overrun_count(&self) {
        self.overrun_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a callback duration in nanoseconds.
    ///
    /// Real-time safe — uses atomics only. Updates min, max, and EMA.
    #[inline(always)]
    pub fn record_callback_duration_nanos(&self, nanos: u64) {
        // update min (atomic min loop)
        let mut prev_min = self.min_callback_nanos.load(Ordering::Relaxed);
        while nanos < prev_min {
            match self.min_callback_nanos.compare_exchange_weak(
                prev_min,
                nanos,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(found) => prev_min = found,
            }
        }

        // update max (atomic max loop)
        let mut prev_max = self.max_callback_nanos.load(Ordering::Relaxed);
        while nanos > prev_max {
            match self.max_callback_nanos.compare_exchange_weak(
                prev_max,
                nanos,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(found) => prev_max = found,
            }
        }

        // update EMA (stored as f64 bits in AtomicU64)
        // EMA_new = alpha * x + (1 - alpha) * EMA_old
        let alpha = self.ema_alpha;
        let mut old_bits = self.ema_callback_bits.load(Ordering::Relaxed);
        loop {
            let old_f = f64::from_bits(old_bits);
            let new_f = alpha * (nanos as f64) + (1.0 - alpha) * old_f;
            let new_bits = new_f.to_bits();
            match self.ema_callback_bits.compare_exchange_weak(
                old_bits,
                new_bits,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(found) => old_bits = found,
            }
        }
    }

    /// Convenience for recording a `Duration`.
    #[inline(always)]
    pub fn record_callback_duration(&self, d: Duration) {
        let nanos = d.as_nanos() as u64;
        self.record_callback_duration_nanos(nanos);
    }

    /// Returns a stack guard that will record the elapsed time between construction
    /// and drop. Useful inside the callback:
    ///
    /// ```ignore
    /// let _g = monitor.scoped_callback();
    /// // ... callback work ...
    /// // timing recorded when `_g` drops
    /// ```
    #[inline(always)]
    pub fn scoped_callback(&self) -> RealtimeGuard<'_> {
        // increment callback count immediately
        self.increment_callback_count();
        let start = self.clock.now(); // quanta::Instant (aliased as QuantaInstant)
        RealtimeGuard {
            monitor: self,
            start,
        }
    }

    // ---------------------------
    // Snapshot (non-RT)
    // ---------------------------

    /// Take a snapshot of the current metrics. If `reset_peaks` is true, the min/max
    /// values will be reset (min -> u64::MAX, max -> 0) after reading so new peaks
    /// are collected from zero.
    ///
    /// This function is NOT real-time safe and should be called from a non-RT thread.
    pub fn snapshot(&mut self, reset_peaks: bool) -> PerformanceSnapshot {
        // read counters
        let frames_processed = self.frames_processed.load(Ordering::Relaxed);
        let callback_count = self.callback_count.load(Ordering::Relaxed);
        let underrun_count = self.underrun_count.load(Ordering::Relaxed);
        let overrun_count = self.overrun_count.load(Ordering::Relaxed);
        let min_raw = self.min_callback_nanos.load(Ordering::Relaxed);
        let max_raw = self.max_callback_nanos.load(Ordering::Relaxed);
        let ema_bits = self.ema_callback_bits.load(Ordering::Relaxed);
        let ema_f = f64::from_bits(ema_bits);
        let expected_callback_nanos = (self.frame_size as f64 / self.sample_rate as f64) * 1_000_000_000.0;
        // load = EMA callback time / expected time
        let avg_load_percent = if expected_callback_nanos > 0.0 {
            (ema_f / expected_callback_nanos) * 100.0
        } else {
            0.0
        };

        // translate optional min/max (u64::MAX indicates "unset")
        let min_callback_nanos = if min_raw == u64::MAX {
            None
        } else {
            Some(min_raw)
        };
        let max_callback_nanos = if max_raw == 0 { None } else { Some(max_raw) };

        // optionally reset peaks (non-RT)
        if reset_peaks {
            self.min_callback_nanos.store(u64::MAX, Ordering::Relaxed);
            self.max_callback_nanos.store(0, Ordering::Relaxed);
            // reset EMA to 0
            self.ema_callback_bits.store(0u64, Ordering::Relaxed);
        }

        PerformanceSnapshot {
            frames_processed,
            callback_count,
            underrun_count,
            overrun_count,
            min_callback_nanos,
            max_callback_nanos,
            ema_callback_nanos: ema_f,
            expected_callback_nanos,
            avg_load_percent,
            timestamp: Instant::now(),
        }
    }

    /// Reset *all* counters (non-RT). Useful when starting a new session or test.
    pub fn reset_all(&mut self) {
        self.frames_processed.store(0, Ordering::Relaxed);
        self.callback_count.store(0, Ordering::Relaxed);
        self.underrun_count.store(0, Ordering::Relaxed);
        self.overrun_count.store(0, Ordering::Relaxed);
        self.min_callback_nanos.store(u64::MAX, Ordering::Relaxed);
        self.max_callback_nanos.store(0, Ordering::Relaxed);
        self.ema_callback_bits.store(0u64, Ordering::Relaxed);
    }
}

/// Small guard that records callback latency on drop. Real-time safe; `Drop` calls
/// atomics on the monitor (no locks, no allocations).
pub struct RealtimeGuard<'a> {
    monitor: &'a PerformanceMonitor,
    start: QuantaInstant,
}

impl<'a> Drop for RealtimeGuard<'a> {
    fn drop(&mut self) {
        let now = self.monitor.clock.now();
        // saturating_duration_since returns a Duration and protects against time going backwards
        let elapsed_dur = now.saturating_duration_since(self.start);
        // convert Duration -> u64 nanos safely (clamp to u64::MAX)
        let elapsed_ns_u128 = elapsed_dur.as_nanos();
        let elapsed = if elapsed_ns_u128 > u128::from(u64::MAX) {
            u64::MAX
        } else {
            elapsed_ns_u128 as u64
        };
        self.monitor.record_callback_duration_nanos(elapsed);
    }
}
