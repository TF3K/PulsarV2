use crate::rt_processing::voice_renderer::AudioSource;

/// Fast pseudo-random number generator for audio applications
/// Uses a linear congruential generator (LCG) for deterministic, fast noise
struct FastRng {
    state: u32,
}

impl FastRng {
    fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed }, // Avoid zero seed
        }
    }
    
    #[inline]
    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(1664525).wrapping_add(1013904223);
        self.state
    }
    
    #[inline]
    fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32) * (1.0 / 4294967296.0) // [0.0, 1.0)
    }
    
    #[inline]
    fn next_bipolar(&mut self) -> f32 {
        // Convert to [-1.0, 1.0] range
        (self.next_f32() - 0.5) * 2.0
    }
}

/// White noise generator - equal energy at all frequencies
pub struct WhiteNoise {
    rng: FastRng,
    amplitude: f32,
    active: bool,
}

impl WhiteNoise {
    pub fn new() -> Self {
        Self {
            rng: FastRng::new(1), // Default deterministic seed
            amplitude: 0.1, // Conservative default for noise
            active: true,
        }
    }
    
    pub fn with_seed(seed: u32) -> Self {
        Self {
            rng: FastRng::new(seed),
            amplitude: 0.1,
            active: true,
        }
    }
    
    pub fn with_amplitude(mut self, amplitude: f32) -> Self {
        self.amplitude = amplitude.clamp(0.0, 1.0);
        self
    }
    
    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude.clamp(0.0, 1.0);
    }
    
    pub fn set_seed(&mut self, seed: u32) {
        self.rng = FastRng::new(seed);
    }
    
    pub fn start(&mut self) {
        self.active = true;
    }
    
    pub fn stop(&mut self) {
        self.active = false;
    }
    
    pub fn amplitude(&self) -> f32 {
        self.amplitude
    }
}

impl AudioSource for WhiteNoise {
    fn fill_buffer(&mut self, output: &mut [f32], _sample_rate: f32, channels: usize, frame_count: usize) {
        if !self.active {
            output.fill(0.0);
            return;
        }
        
        for frame_idx in 0..frame_count {
            let sample = self.rng.next_bipolar() * self.amplitude;
            
            let start = frame_idx * channels;
            let end = start + channels;
            for out in &mut output[start..end] {
                *out = sample;
            }
        }
    }
    
    fn is_active(&self) -> bool {
        self.active
    }
    
    fn reset(&mut self) {
        self.rng = FastRng::new(1);
        self.active = true;
    }
}

/// Pink noise generator - 1/f noise, equal energy per octave
/// Approximated using multiple white noise sources at different frequencies
pub struct PinkNoise {
    // Multiple white noise generators for pink noise approximation
    generators: [WhiteNoise; 7],
    coefficients: [f32; 7],
    amplitude: f32,
    active: bool,
}

impl PinkNoise {
    pub fn new() -> Self {
        // Create multiple white noise generators with different seeds
        let generators = [
            WhiteNoise::with_seed(12345),
            WhiteNoise::with_seed(23456),
            WhiteNoise::with_seed(34567),
            WhiteNoise::with_seed(45678),
            WhiteNoise::with_seed(56789),
            WhiteNoise::with_seed(67890),
            WhiteNoise::with_seed(78901),
        ];
        
        // Coefficients for pink noise approximation
        let coefficients = [
            0.049922035, 0.990566037, 0.115926437,
            0.923311349, 0.972852432, 0.063612432,
            0.999981195,
        ];
        
        Self {
            generators,
            coefficients,
            amplitude: 0.1,
            active: true,
        }
    }
    
    pub fn with_amplitude(mut self, amplitude: f32) -> Self {
        self.amplitude = amplitude.clamp(0.0, 1.0);
        self
    }
    
    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude.clamp(0.0, 1.0);
    }
    
    pub fn start(&mut self) {
        self.active = true;
        for generator in &mut self.generators {
            generator.start();
        }
    }
    
    pub fn stop(&mut self) {
        self.active = false;
        for generator in &mut self.generators {
            generator.stop();
        }
    }
    
    pub fn amplitude(&self) -> f32 {
        self.amplitude
    }
}

impl AudioSource for PinkNoise {
    fn fill_buffer(&mut self, output: &mut [f32], sample_rate: f32, channels: usize, frame_count: usize) {
        if !self.active {
            output.fill(0.0);
            return;
        }
        output.fill(0.0);

        for (i, generator) in self.generators.iter_mut().enumerate() {
            let coefficient = self.coefficients[i];
            let mut temp = vec![0.0f32; output.len()];
            generator.fill_buffer(&mut temp, sample_rate, channels, frame_count);

            for (out, &t) in output.iter_mut().zip(&temp) {
                *out += t * coefficient;
            }
        }

        let normalization = 0.11;
        for s in output.iter_mut() {
            *s *= self.amplitude * normalization;
        }
    }
    
    fn is_active(&self) -> bool {
        self.active
    }
    
    fn reset(&mut self) {
        for generator in &mut self.generators {
            generator.reset();
        }
        self.active = true;
    }
}

/// Brown noise generator (Brownian noise) - 1/f² noise
/// Lower frequencies have more energy than pink noise
pub struct BrownNoise {
    rng: FastRng,
    previous_sample: f32,
    amplitude: f32,
    active: bool,
}

impl BrownNoise {
    pub fn new() -> Self {
        Self {
            rng: FastRng::new(9876),
            previous_sample: 0.0,
            amplitude: 0.05, // Even more conservative for brown noise
            active: true,
        }
    }
    
    pub fn with_seed(seed: u32) -> Self {
        Self {
            rng: FastRng::new(seed),
            previous_sample: 0.0,
            amplitude: 0.05,
            active: true,
        }
    }
    
    pub fn with_amplitude(mut self, amplitude: f32) -> Self {
        self.amplitude = amplitude.clamp(0.0, 1.0);
        self
    }
    
    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude.clamp(0.0, 1.0);
    }
    
    pub fn set_seed(&mut self, seed: u32) {
        self.rng = FastRng::new(seed);
        self.previous_sample = 0.0;
    }
    
    pub fn start(&mut self) {
        self.active = true;
    }
    
    pub fn stop(&mut self) {
        self.active = false;
    }
    
    pub fn amplitude(&self) -> f32 {
        self.amplitude
    }
}

impl AudioSource for BrownNoise {
    fn fill_buffer(&mut self, output: &mut [f32], _sample_rate: f32, channels: usize, frame_count: usize) {
        if !self.active {
            output.fill(0.0);
            return;
        }
        
        for frame_idx in 0..frame_count {
            // Brown noise is integrated white noise
            let white_sample = self.rng.next_bipolar() * 0.1; // Small step size
            self.previous_sample += white_sample;
            
            // Prevent drift by applying a small leak
            self.previous_sample *= 0.9999;
            
            // Clamp to prevent overflow
            self.previous_sample = self.previous_sample.clamp(-1.0, 1.0);
            
            let sample = self.previous_sample * self.amplitude;
            
            let start = frame_idx * channels;
            let end = start + channels;
            for out in &mut output[start..end] {
                *out = sample;
            }
        }
    }
    
    fn is_active(&self) -> bool {
        self.active
    }
    
    fn reset(&mut self) {
        self.rng = FastRng::new(9876);
        self.previous_sample = 0.0;
        self.active = true;
    }
}

/// Burst noise generator - random bursts of noise
pub struct BurstNoise {
    rng: FastRng,
    burst_probability: f32, // Probability of burst per sample (0.0 to 1.0)
    burst_duration: u32,    // Current burst duration in samples
    burst_counter: u32,     // Current position in burst
    amplitude: f32,
    active: bool,
}

impl BurstNoise {
    pub fn new() -> Self {
        Self {
            rng: FastRng::new(5432),
            burst_probability: 0.001, // 0.1% chance per sample
            burst_duration: 0,
            burst_counter: 0,
            amplitude: 0.2,
            active: true,
        }
    }
    
    pub fn with_burst_probability(mut self, probability: f32) -> Self {
        self.burst_probability = probability.clamp(0.0, 1.0);
        self
    }
    
    pub fn with_amplitude(mut self, amplitude: f32) -> Self {
        self.amplitude = amplitude.clamp(0.0, 1.0);
        self
    }
    
    pub fn set_burst_probability(&mut self, probability: f32) {
        self.burst_probability = probability.clamp(0.0, 1.0);
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
    
    pub fn amplitude(&self) -> f32 {
        self.amplitude
    }
}

impl AudioSource for BurstNoise {
    fn fill_buffer(&mut self, output: &mut [f32], _sample_rate: f32, channels: usize, frame_count: usize) {
        if !self.active {
            output.fill(0.0);
            return;
        }
        
        for frame_idx in 0..frame_count {
            let mut sample = 0.0;
            
            // Check if we're in a burst
            if self.burst_counter > 0 {
                sample = self.rng.next_bipolar() * self.amplitude;
                self.burst_counter -= 1;
            } else {
                // Check if we should start a new burst
                if self.rng.next_f32() < self.burst_probability {
                    // Start new burst with random duration (10-1000 samples)
                    self.burst_duration = 10 + ((self.rng.next_f32() * 990.0) as u32);
                    self.burst_counter = self.burst_duration;
                    sample = self.rng.next_bipolar() * self.amplitude;
                }
            }
            
            let start = frame_idx * channels;
            let end = start + channels;
            for out in &mut output[start..end] {
                *out = sample;
            }
        }
    }
    
    fn is_active(&self) -> bool {
        self.active
    }
    
    fn reset(&mut self) {
        self.rng = FastRng::new(5432);
        self.burst_duration = 0;
        self.burst_counter = 0;
        self.active = true;
    }
}