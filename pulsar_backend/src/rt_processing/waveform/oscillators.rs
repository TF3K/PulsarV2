use crate::rt_processing::voice_renderer::AudioSource;
use super::tables::{WaveformType, normalize_phase, phase_increment, init_tables};
use crossbeam::atomic::AtomicCell;

/// A versatile oscillator that can generate multiple waveform types
pub struct Oscillator {
    waveform: WaveformType,
    frequency: f32,
    amplitude: f32,
    phase: AtomicCell<f32>,
    active: bool,
    use_interpolation: bool,
}

impl Oscillator {
    /// Create a new oscillator with specified waveform and frequency
    pub fn new(waveform: WaveformType, frequency: f32) -> Self {
        // Ensure tables are initialized
        init_tables();
        
        Self {
            waveform,
            frequency,
            amplitude: 0.5, // Safe default volume
            phase: AtomicCell::new(0.0),
            active: true,
            use_interpolation: true, // High quality by default
        }
    }

    pub fn next_sample(&mut self, sample_rate: f32) -> f32 {
        let phase_inc = phase_increment(self.frequency, sample_rate);
        let mut current_phase = self.phase.load();
        let sample = if self.use_interpolation {
            self.waveform.interpolated_sample(current_phase)
        } else {
            self.waveform.fast_sample(current_phase)
        } * self.amplitude;
        current_phase += phase_inc;
        self.phase.store(normalize_phase(current_phase));
        sample
    }
    
    /// Create a sine wave oscillator
    pub fn sine(frequency: f32) -> Self {
        Self::new(WaveformType::Sine, frequency)
    }
    
    /// Create a triangle wave oscillator
    pub fn triangle(frequency: f32) -> Self {
        Self::new(WaveformType::Triangle, frequency)
    }
    
    /// Create a sawtooth wave oscillator
    pub fn sawtooth(frequency: f32) -> Self {
        Self::new(WaveformType::Sawtooth, frequency)
    }
    
    /// Create a square wave oscillator
    pub fn square(frequency: f32) -> Self {
        Self::new(WaveformType::Square, frequency)
    }
    
    /// Set the amplitude (volume) of the oscillator
    pub fn with_amplitude(mut self, amplitude: f32) -> Self {
        self.amplitude = amplitude.clamp(0.0, 1.0);
        self
    }
    
    /// Enable or disable interpolation (trade quality for performance)
    pub fn with_interpolation(mut self, use_interpolation: bool) -> Self {
        self.use_interpolation = use_interpolation;
        self
    }
    
    /// Set starting phase (0.0 to 1.0)
    pub fn with_phase(self, phase: f32) -> Self {
        self.phase.store(normalize_phase(phase));
        self
    }
    
    // Setters for runtime modification
    
    pub fn set_waveform(&mut self, waveform: WaveformType) {
        self.waveform = waveform;
    }
    
    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency;
    }
    
    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude.clamp(0.0, 1.0);
    }
    
    pub fn set_phase(&mut self, phase: f32) {
        self.phase.store(normalize_phase(phase));
    }
    
    pub fn set_interpolation(&mut self, use_interpolation: bool) {
        self.use_interpolation = use_interpolation;
    }
    
    // Getters
    
    pub fn waveform(&self) -> WaveformType {
        self.waveform
    }
    
    pub fn frequency(&self) -> f32 {
        self.frequency
    }
    
    pub fn amplitude(&self) -> f32 {
        self.amplitude
    }
    
    pub fn current_phase(&self) -> f32 {
        self.phase.load()
    }
    
    // Control methods
    
    pub fn start(&mut self) {
        self.active = true;
    }
    
    pub fn stop(&mut self) {
        self.active = false;
    }
    
    pub fn toggle(&mut self) {
        self.active = !self.active;
    }
}

impl AudioSource for Oscillator {
    fn fill_buffer(&mut self, output: &mut [f32], sample_rate: f32, channels: usize, frame_count: usize) {
        if !self.active {
            output.fill(0.0);
            return;
        }
        
        let phase_inc = phase_increment(self.frequency, sample_rate);
        let mut current_phase = self.phase.load();
        
        for frame_idx in 0..frame_count {
            // Generate sample based on waveform type and quality setting
            let sample = if self.use_interpolation {
                self.waveform.interpolated_sample(current_phase)
            } else {
                self.waveform.fast_sample(current_phase)
            } * self.amplitude;
            
            // Fill all channels for this frame with the same sample
            let start = frame_idx * channels;
            let end = start + channels;
            for out in &mut output[start..end] {
                *out = sample;
            }
            
            current_phase += phase_inc;
        }
        
        // Normalize phase to prevent accumulation errors
        current_phase = normalize_phase(current_phase);
        self.phase.store(current_phase);
    }
    
    fn is_active(&self) -> bool {
        self.active
    }
    
    fn reset(&mut self) {
        self.phase.store(0.0);
        self.active = true;
    }
}

/// A specialized sine wave oscillator for maximum performance
/// Uses the optimized sine table from your original implementation
pub struct SineOscillator {
    frequency: f32,
    amplitude: f32,
    phase: AtomicCell<f32>,
    active: bool,
}

impl SineOscillator {
    pub fn new(frequency: f32) -> Self {
        init_tables();
        
        Self {
            frequency,
            amplitude: 0.5,
            phase: AtomicCell::new(0.0),
            active: true,
        }
    }
    
    pub fn with_amplitude(mut self, amplitude: f32) -> Self {
        self.amplitude = amplitude.clamp(0.0, 1.0);
        self
    }
    
    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency;
    }
    
    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude.clamp(0.0, 1.0);
    }
    
    pub fn start(&mut self) {
        self.active = true;
    }
    
    pub fn stop(&mut self) {
        self.active = false;
    }
    
    pub fn frequency(&self) -> f32 {
        self.frequency
    }
    
    pub fn amplitude(&self) -> f32 {
        self.amplitude
    }
}

impl AudioSource for SineOscillator {
    fn fill_buffer(&mut self, output: &mut [f32], sample_rate: f32, channels: usize, frame_count: usize) {
        if !self.active {
            output.fill(0.0);
            return;
        }
        
        let phase_inc = phase_increment(self.frequency, sample_rate);
        let mut current_phase = self.phase.load();
        
        for frame_idx in 0..frame_count {
            let sample = WaveformType::Sine.interpolated_sample(current_phase) * self.amplitude;
            
            let start = frame_idx * channels;
            let end = start + channels;
            for out in &mut output[start..end] {
                *out = sample;
            }
            
            current_phase += phase_inc;
        }
        
        current_phase = normalize_phase(current_phase);
        self.phase.store(current_phase);
    }
    
    fn is_active(&self) -> bool {
        self.active
    }
    
    fn reset(&mut self) {
        self.phase.store(0.0);
        self.active = true;
    }
}

/// An LFO (Low Frequency Oscillator) for modulation purposes
/// Typically used for vibrato, tremolo, filter sweeps, etc.
pub struct LFO {
    oscillator: Oscillator,
    depth: f32,
    offset: f32,
}

impl LFO {
    /// Create a new LFO with specified waveform and frequency (usually < 20 Hz)
    pub fn new(waveform: WaveformType, frequency: f32) -> Self {
        Self {
            oscillator: Oscillator::new(waveform, frequency).with_amplitude(1.0),
            depth: 1.0,
            offset: 0.0,
        }
    }
    
    /// Set the modulation depth (0.0 to 1.0)
    pub fn with_depth(mut self, depth: f32) -> Self {
        self.depth = depth.clamp(0.0, 1.0);
        self
    }
    
    /// Set the DC offset (-1.0 to 1.0)
    pub fn with_offset(mut self, offset: f32) -> Self {
        self.offset = offset.clamp(-1.0, 1.0);
        self
    }
    
    /// Get the current LFO value for modulation
    pub fn get_value(&mut self, sample_rate: f32) -> f32 {
        (self.oscillator.next_sample(sample_rate) * self.depth) + self.offset
    }

    
    pub fn set_frequency(&mut self, frequency: f32) {
        self.oscillator.set_frequency(frequency);
    }
    
    pub fn set_depth(&mut self, depth: f32) {
        self.depth = depth.clamp(0.0, 1.0);
    }
    
    pub fn set_offset(&mut self, offset: f32) {
        self.offset = offset.clamp(-1.0, 1.0);
    }
    
    pub fn start(&mut self) {
        self.oscillator.start();
    }
    
    pub fn stop(&mut self) {
        self.oscillator.stop();
    }
}