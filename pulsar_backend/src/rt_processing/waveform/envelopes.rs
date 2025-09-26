use crate::rt_processing::voice_renderer::AudioSource;

/// ADSR envelope states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnvelopeState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
    Finished,
}

/// ADSR (Attack, Decay, Sustain, Release) envelope generator
#[derive(Debug, Clone)]
pub struct ADSREnvelope {
    // Timing parameters (in seconds)
    attack_time: f32,
    decay_time: f32,
    sustain_level: f32,  // 0.0 to 1.0
    release_time: f32,
    
    // Current state
    state: EnvelopeState,
    current_value: f32,
    sample_rate: f32,
    
    // Internal counters (in samples)
    attack_samples: u32,
    decay_samples: u32,
    release_samples: u32,
    current_sample: u32,
    
    // Note control
    note_on: bool,
    note_off_triggered: bool,
}

impl ADSREnvelope {
    /// Create a new ADSR envelope with specified parameters
    pub fn new(attack_time: f32, decay_time: f32, sustain_level: f32, release_time: f32) -> Self {
        Self {
            attack_time,
            decay_time,
            sustain_level: sustain_level.clamp(0.0, 1.0),
            release_time,
            state: EnvelopeState::Idle,
            current_value: 0.0,
            sample_rate: 44100.0, // Default, will be updated on first use
            attack_samples: 0,
            decay_samples: 0,
            release_samples: 0,
            current_sample: 0,
            note_on: false,
            note_off_triggered: false,
        }
    }
    
    /// Create a quick envelope for testing
    pub fn quick() -> Self {
        Self::new(0.01, 0.1, 0.7, 0.3) // 10ms attack, 100ms decay, 70% sustain, 300ms release
    }
    
    /// Create a slow envelope for pads
    pub fn slow() -> Self {
        Self::new(1.0, 0.5, 0.8, 2.0) // 1s attack, 500ms decay, 80% sustain, 2s release
    }
    
    /// Create a percussive envelope (no sustain)
    pub fn percussive() -> Self {
        Self::new(0.01, 0.2, 0.0, 0.1) // Quick attack, 200ms decay to silence, quick release
    }
    
    /// Trigger note on
    pub fn note_on(&mut self) {
        self.note_on = true;
        self.note_off_triggered = false;
        self.state = EnvelopeState::Attack;
        self.current_sample = 0;
        self.update_sample_counts();
    }
    
    /// Trigger note off
    pub fn note_off(&mut self) {
        if self.note_on && !self.note_off_triggered {
            self.note_on = false;
            self.note_off_triggered = true;
            self.state = EnvelopeState::Release;
            self.current_sample = 0;
        }
    }
    
    /// Get the current envelope value (0.0 to 1.0)
    pub fn get_value(&mut self, sample_rate: f32) -> f32 {
        if self.sample_rate != sample_rate {
            self.sample_rate = sample_rate;
            self.update_sample_counts();
        }
        
        self.process_sample();
        self.current_value
    }
    
    /// Check if the envelope is active (not idle or finished)
    pub fn is_active(&self) -> bool {
        !matches!(self.state, EnvelopeState::Idle | EnvelopeState::Finished)
    }
    
    /// Check if the envelope is finished
    pub fn is_finished(&self) -> bool {
        self.state == EnvelopeState::Finished
    }
    
    /// Get current envelope state
    pub fn state(&self) -> EnvelopeState {
        self.state
    }
    
    /// Reset envelope to idle state
    pub fn reset(&mut self) {
        self.state = EnvelopeState::Idle;
        self.current_value = 0.0;
        self.current_sample = 0;
        self.note_on = false;
        self.note_off_triggered = false;
    }
    
    // Setters for runtime modification
    pub fn set_attack_time(&mut self, attack_time: f32) {
        self.attack_time = attack_time;
        self.update_sample_counts();
    }
    
    pub fn set_decay_time(&mut self, decay_time: f32) {
        self.decay_time = decay_time;
        self.update_sample_counts();
    }
    
    pub fn set_sustain_level(&mut self, sustain_level: f32) {
        self.sustain_level = sustain_level.clamp(0.0, 1.0);
    }
    
    pub fn set_release_time(&mut self, release_time: f32) {
        self.release_time = release_time;
        self.update_sample_counts();
    }
    
    // Getters
    pub fn attack_time(&self) -> f32 { self.attack_time }
    pub fn decay_time(&self) -> f32 { self.decay_time }
    pub fn sustain_level(&self) -> f32 { self.sustain_level }
    pub fn release_time(&self) -> f32 { self.release_time }
    
    // Internal methods
    fn update_sample_counts(&mut self) {
        self.attack_samples = (self.attack_time * self.sample_rate) as u32;
        self.decay_samples = (self.decay_time * self.sample_rate) as u32;
        self.release_samples = (self.release_time * self.sample_rate) as u32;
    }
    
    fn process_sample(&mut self) {
        match self.state {
            EnvelopeState::Idle => {
                self.current_value = 0.0;
            }
            
            EnvelopeState::Attack => {
                if self.attack_samples == 0 {
                    self.current_value = 1.0;
                    self.state = EnvelopeState::Decay;
                    self.current_sample = 0;
                } else {
                    self.current_value = self.current_sample as f32 / self.attack_samples as f32;
                    self.current_sample += 1;
                    
                    if self.current_sample >= self.attack_samples {
                        self.current_value = 1.0;
                        self.state = EnvelopeState::Decay;
                        self.current_sample = 0;
                    }
                }
            }
            
            EnvelopeState::Decay => {
                if self.decay_samples == 0 {
                    self.current_value = self.sustain_level;
                    self.state = EnvelopeState::Sustain;
                } else {
                    let progress = self.current_sample as f32 / self.decay_samples as f32;
                    self.current_value = 1.0 - (progress * (1.0 - self.sustain_level));
                    self.current_sample += 1;
                    
                    if self.current_sample >= self.decay_samples {
                        self.current_value = self.sustain_level;
                        self.state = EnvelopeState::Sustain;
                    }
                }
            }
            
            EnvelopeState::Sustain => {
                self.current_value = self.sustain_level;
                // Stay in sustain until note off
            }
            
            EnvelopeState::Release => {
                if self.release_samples == 0 {
                    self.current_value = 0.0;
                    self.state = EnvelopeState::Finished;
                } else {
                    let start_level = if self.note_off_triggered {
                        self.current_value // Start release from current level
                    } else {
                        self.sustain_level
                    };
                    
                    let progress = self.current_sample as f32 / self.release_samples as f32;
                    self.current_value = start_level * (1.0 - progress);
                    self.current_sample += 1;
                    
                    if self.current_sample >= self.release_samples {
                        self.current_value = 0.0;
                        self.state = EnvelopeState::Finished;
                    }
                }
            }
            
            EnvelopeState::Finished => {
                self.current_value = 0.0;
            }
        }
    }
}

/// A wrapper that applies an ADSR envelope to any AudioSource
pub struct EnvelopedSource {
    source: Box<dyn AudioSource + Send>,
    envelope: ADSREnvelope,
    auto_retrigger: bool, // Automatically trigger note_on when source becomes active
}

impl EnvelopedSource {
    pub fn new(source: Box<dyn AudioSource>, envelope: ADSREnvelope) -> Self {
        Self {
            source,
            envelope,
            auto_retrigger: true,
        }
    }
    
    pub fn with_auto_retrigger(mut self, auto_retrigger: bool) -> Self {
        self.auto_retrigger = auto_retrigger;
        self
    }
    
    /// Manually trigger the envelope
    pub fn note_on(&mut self) {
        self.envelope.note_on();
    }
    
    pub fn note_off(&mut self) {
        self.envelope.note_off();
    }
    
    /// Get mutable reference to the envelope for parameter changes
    pub fn envelope_mut(&mut self) -> &mut ADSREnvelope {
        &mut self.envelope
    }
    
    /// Get reference to the wrapped audio source
    pub fn source_mut(&mut self) -> &mut Box<dyn AudioSource + Send> {
        &mut self.source
    }
}

impl AudioSource for EnvelopedSource {
    fn fill_buffer(&mut self, output: &mut [f32], sample_rate: f32, channels: usize, frame_count: usize) {
        // Auto-trigger if enabled and source becomes active
        if self.auto_retrigger && self.source.is_active() && !self.envelope.is_active() {
            self.envelope.note_on();
        }
        
        // Get audio from wrapped source
        self.source.fill_buffer(output, sample_rate, channels, frame_count);
        
        // Apply envelope to each frame
        for frame_idx in 0..frame_count {
            let envelope_value = self.envelope.get_value(sample_rate);
            
            let start = frame_idx * channels;
            let end = start + channels;
            for sample in &mut output[start..end] {
                *sample *= envelope_value;
            }
        }
        
        // If envelope is finished and we're not auto-retriggering, the source should stop
        if self.envelope.is_finished() && !self.auto_retrigger {
            // This allows the envelope to control the source lifetime
        }
    }
    
    fn is_active(&self) -> bool {
        self.source.is_active() && (self.envelope.is_active() || self.auto_retrigger)
    }
    
    fn reset(&mut self) {
        self.source.reset();
        self.envelope.reset();
    }
}

/// Simple linear envelope for quick fades
pub struct LinearEnvelope {
    start_value: f32,
    end_value: f32,
    duration_samples: u32,
    current_sample: u32,
    current_value: f32,
    finished: bool,
}

impl LinearEnvelope {
    pub fn new(start_value: f32, end_value: f32, duration_seconds: f32, sample_rate: f32) -> Self {
        let duration_samples = (duration_seconds * sample_rate) as u32;
        
        Self {
            start_value,
            end_value,
            duration_samples,
            current_sample: 0,
            current_value: start_value,
            finished: false,
        }
    }
    
    /// Create a fade-in envelope
    pub fn fade_in(duration_seconds: f32, sample_rate: f32) -> Self {
        Self::new(0.0, 1.0, duration_seconds, sample_rate)
    }
    
    /// Create a fade-out envelope
    pub fn fade_out(duration_seconds: f32, sample_rate: f32) -> Self {
        Self::new(1.0, 0.0, duration_seconds, sample_rate)
    }
    
    pub fn get_value(&mut self) -> f32 {
        if self.finished {
            return self.current_value;
        }
        
        if self.current_sample >= self.duration_samples {
            self.current_value = self.end_value;
            self.finished = true;
        } else {
            let progress = self.current_sample as f32 / self.duration_samples as f32;
            self.current_value = self.start_value + (progress * (self.end_value - self.start_value));
            self.current_sample += 1;
        }
        
        self.current_value
    }
    
    pub fn is_finished(&self) -> bool {
        self.finished
    }
    
    pub fn reset(&mut self) {
        self.current_sample = 0;
        self.current_value = self.start_value;
        self.finished = false;
    }
}