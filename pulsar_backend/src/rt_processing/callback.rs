//! Lock-conscious realtime audio callback slot.
//!
//! Design goals:
//! - Avoid OS mutex/syscall in the hot audio callback path.
//! - Allow hot-swapping the processing engine from another thread.
//! - Never allocate inside the audio thread.
//! - If processor is unavailable (locked), output silence to avoid glitches.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use spin::Mutex; // small, in-process spinning lock good for realtime callbacks

/// Trait every realtime processor must implement.
///
/// NOTE: `process` receives a mutable reference and must not perform blocking operations.
/// Implementations should avoid heavy allocations inside `process`.
pub trait AudioCallback: Send + 'static {
    /// Fill the interleaved `output` buffer (length == frames * channels) with audio.
    ///
    /// - `output`: interleaved f32 buffer to fill (already sized by caller).
    /// - `sample_rate`: sample rate in Hz.
    /// - `channels`: number of channels (e.g., 2 for stereo).
    /// - `frames`: number of frames in this buffer.
    fn process(&mut self, output: &mut [f32], sample_rate: f32, channels: usize, frames: usize);
}

/// A wrapper that holds a processor and provides a realtime-safe `process` entrypoint.
///
/// Internally it holds `Arc<spin::Mutex<Box<dyn AudioCallback>>>`. In the audio thread we
/// attempt a non-blocking `try_lock`. If the lock cannot be obtained quickly, we zero
/// the output buffer (silence) to avoid blocking the audio thread.
///
/// The wrapper also holds an atomic sample counter for playback position/monitoring.
pub struct CallbackSlot {
    /// Processor slot (hot-swappable). Use `spin::Mutex` to avoid OS-level blocking.
    processor: Arc<Mutex<Box<dyn AudioCallback>>>,

    /// Sample clock (frames processed). Atomic so it can be read from other threads.
    sample_clock: Arc<AtomicU64>,

    /// Current sample rate & channels used for the audio thread. These are read-only from
    /// the audio thread side; updates to them should be done with `set_runtime_config`.
    sample_rate: f32,
    channels: usize,
}

impl CallbackSlot {
    /// Create a new slot wrapping a processor.
    ///
    /// `initial_processor` must be a boxed object implementing `AudioCallback`.
    /// `sample_rate` and `channels` describe the runtime used by the audio thread.
    pub fn new(initial_processor: Box<dyn AudioCallback>, sample_rate: f32, channels: usize) -> Self {
        Self {
            processor: Arc::new(Mutex::new(initial_processor)),
            sample_clock: Arc::new(AtomicU64::new(0)),
            sample_rate,
            channels,
        }
    }

    /// Replaces the current processor with a new one.
    ///
    /// This attempts to acquire the lock and swap. If the lock is briefly contended,
    /// we spin until we can swap it — swapping is expected to be infrequent and fast.
    pub fn swap_processor(&self, new_processor: Box<dyn AudioCallback>) {
        let mut guard = self.processor.lock();
        *guard = new_processor;
        // lock released on drop
    }

    /// Try to mutate the processor in-place using a closure.
    ///
    /// Useful to change parameters without replacing the whole boxed object.
    /// This will block (spin) until the lock is acquired.
    pub fn with_processor_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Box<dyn AudioCallback>) -> R,
    {
        let mut guard = self.processor.lock();
        f(&mut guard)
    }

    /// Realtime-safe process entry called from the audio I/O callback.
    ///
    /// - `output` is an interleaved f32 buffer (frames * channels long).
    /// - Returns `true` if the processor ran; `false` if we fell back to silence.
    ///
    /// **Important**: This method performs no heap allocation.
    pub fn process_realtime(&self, output: &mut [f32]) -> bool {
        // Guard: output buffer length must be divisible by channels.
        let frames = match output.len() / self.channels {
            0 => return false, // nothing to do
            n => n,
        };

        // Advance sample clock (frames, not samples).
        // We store frame count so playback_time is frames / sample_rate.
        self.sample_clock.fetch_add(frames as u64, Ordering::Relaxed);

        // Try to acquire the processor lock without blocking the OS.
        // spin::Mutex::try_lock() exists but isn't stable on all versions; we use lock() which spins briefly.
        // To be extra-safe against long blocking we can attempt a quick spin approach:
        //
        //   if let Some(mut guard) = self.processor.try_lock() { ... } else { silence; return false; }
        //
        // spin::Mutex currently provides try_lock() returning Option, so we can use it.
        if let Some(mut guard) = self.processor.try_lock() {
            // Processor exists; call its process method.
            // Implementations MUST NOT block or allocate here.
            guard.process(output, self.sample_rate, self.channels, frames);
            true
        } else {
            // Could not lock quickly — output silence to avoid glitches.
            output.fill(0.0);
            false
        }
    }

    /// Get current playback time in seconds (frames / sample_rate).
    pub fn playback_time(&self) -> f32 {
        let frames = self.sample_clock.load(Ordering::Relaxed);
        (frames as f32) / self.sample_rate
    }

    /// Get raw frame count processed so far.
    pub fn frame_count(&self) -> u64 {
        self.sample_clock.load(Ordering::Relaxed)
    }

    /// Return a cloneable handle to the internal processor Arc. This allows other parts
    /// of the program to hold a reference if needed.
    pub fn processor_handle(&self) -> Arc<Mutex<Box<dyn AudioCallback>>> {
        Arc::clone(&self.processor)
    }

    /// Update sample_rate and channels. Call from non-realtime thread only.
    ///
    /// NOTE: Audio thread must be restarted or guaranteed to use the new values before next callback.
    pub fn set_runtime_config(&mut self, sample_rate: f32, channels: usize) {
        self.sample_rate = sample_rate;
        self.channels = channels;
    }

    /// Convenience: create a `CallbackSlot` that uses a no-op silent processor.
    pub fn silent(sample_rate: f32, channels: usize) -> Self {
        Self::new(Box::new(SilentProcessor {}), sample_rate, channels)
    }
}

/// A trivial silent processor implementation.
struct SilentProcessor {}

impl AudioCallback for SilentProcessor {
    fn process(&mut self, output: &mut [f32], _sample_rate: f32, _channels: usize, _frames: usize) {
        output.fill(0.0);
    }
}
