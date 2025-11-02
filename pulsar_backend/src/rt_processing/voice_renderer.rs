use crate::rt_processing::routing::{AudioSource as RoutingAudioSource, Router, Pan, PanLaw};
use crate::rt_processing::callback::AudioCallback;

/// Trait for waveform generators that produce audio samples
/// This is our internal waveform interface - simpler than the routing interface
pub trait AudioSource: Send + Sync {
    /// Fill the output buffer with audio samples (interleaved if multi-channel)
    fn fill_buffer(&mut self, output: &mut [f32], sample_rate: f32, channels: usize, frame_count: usize);

    /// Check if this audio source is still active/playing
    fn is_active(&self) -> bool;

    /// Reset the audio source to its initial state
    fn reset(&mut self);
}

/// Adapter that bridges our waveform AudioSource to the routing AudioSource
struct WaveformAdapter<T: AudioSource> {
    source: T,
    temp_buffer: Vec<f32>, // interleaved temp buffer
}

impl<T: AudioSource> WaveformAdapter<T> {
    fn new(source: T) -> Self {
        Self {
            source,
            temp_buffer: Vec::new(),
        }
    }
}

impl<T: AudioSource> RoutingAudioSource for WaveformAdapter<T> {
    fn render(&mut self, output: &mut [&mut [f32]], frames: usize, sample_rate: f32) {
        let channels = output.len();

        // Resize temp buffer if needed (interleaved)
        let needed_size = frames * channels;
        if self.temp_buffer.len() < needed_size {
            self.temp_buffer.resize(needed_size, 0.0);
        }

        // Fill interleaved temp buffer using our waveform interface
        self.source.fill_buffer(&mut self.temp_buffer[..needed_size], sample_rate, channels, frames);

        // De-interleave into non-interleaved output for routing system
        for frame in 0..frames {
            for ch in 0..channels {
                output[ch][frame] = self.temp_buffer[frame * channels + ch];
            }
        }
    }
}

/// Voice processor that integrates with the real-time callback system
pub struct VoiceProcessor {
    router: Router,
    _temp_interleaved: Vec<f32>,
    next_source_id: usize,
}

impl VoiceProcessor {
    /// Create a new voice processor
    pub fn new(channels: usize, sample_rate: f32, max_frames: usize, num_buses: usize) -> Self {
        Self {
            router: Router::new(channels, sample_rate, num_buses.max(1), max_frames),
            _temp_interleaved: Vec::with_capacity(max_frames * channels),
            next_source_id: 0,
        }
    }

    /// Create a basic stereo voice processor with 4 buses
    pub fn stereo(sample_rate: f32, max_frames: usize) -> Self {
        Self::new(2, sample_rate, max_frames, 4)
    }

    /// Add a waveform audio source to the processor
    pub fn add_waveform_source<T: AudioSource + 'static>(
        &mut self,
        source: T,
        gain: f32,
        pan: f32,
        bus: usize
    ) -> usize {
        let pan_control = Pan {
            value: pan.clamp(-1.0, 1.0),
            law: PanLaw::EqualPower,
        };

        let adapter = WaveformAdapter::new(source);
        // Coerce into the routing trait object (requires 'static; we bound T with 'static)
        self.router.add_source(Box::new(adapter), gain, pan_control, bus);

        let id = self.next_source_id;
        self.next_source_id += 1;
        id
    }

    /// Add a routing audio source directly (for advanced use)
    pub fn add_routing_source(
        &mut self,
        source: Box<dyn RoutingAudioSource + 'static>,
        gain: f32,
        pan: Pan,
        bus: usize
    ) -> usize {
        self.router.add_source(source, gain, pan, bus);

        let id = self.next_source_id;
        self.next_source_id += 1;
        id
    }

    /// Clear all sources
    pub fn clear_sources(&mut self) {
        self.router.clear_sources();
    }

    /// Get access to the internal router for advanced operations
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Get mutable access to the internal router for advanced operations
    pub fn router_mut(&mut self) -> &mut Router {
        &mut self.router
    }
}

impl AudioCallback for VoiceProcessor {
    fn process(&mut self, output: &mut [f32], _sample_rate: f32, _channels: usize, _frames: usize) {
        // The router handles all the processing - just delegate to it
        // It will handle mixing, panning, bus routing, etc.
        self.router.process(output, None);
    }
}

/// A simple test audio source that generates silence
pub struct SilenceSource;

impl AudioSource for SilenceSource {
    fn fill_buffer(&mut self, output: &mut [f32], _sample_rate: f32, _channels: usize, _frame_count: usize) {
        output.fill(0.0);
    }

    fn is_active(&self) -> bool {
        true // Always active, just generates silence
    }

    fn reset(&mut self) {
        // Nothing to reset for silence
    }
}

/// A simple test source that generates a test tone
pub struct TestToneSource {
    frequency: f32,
    phase: f32,
    amplitude: f32,
}

impl TestToneSource {
    pub fn new(frequency: f32, amplitude: f32) -> Self {
        Self {
            frequency,
            phase: 0.0,
            amplitude: amplitude.clamp(0.0, 1.0),
        }
    }
}

impl AudioSource for TestToneSource {
    fn fill_buffer(&mut self, output: &mut [f32], sample_rate: f32, channels: usize, frame_count: usize) {
        let phase_increment = self.frequency / sample_rate;

        for frame in 0..frame_count {
            let sample = (self.phase * 2.0 * std::f32::consts::PI).sin() * self.amplitude;

            // Fill all channels with the same sample
            for ch in 0..channels {
                output[frame * channels + ch] = sample;
            }

            self.phase += phase_increment;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
        }
    }

    fn is_active(&self) -> bool {
        true
    }

    fn reset(&mut self) {
        self.phase = 0.0;
    }
}
