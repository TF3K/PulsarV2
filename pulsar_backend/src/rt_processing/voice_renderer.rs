use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

/// Trait for any audio source that can generate samples
pub trait AudioSource: Send + Sync {
    /// Fill the output buffer with audio samples
    /// - `output`: Buffer to fill with samples (interleaved if multi-channel)
    /// - `sample_rate`: Current sample rate
    /// - `channels`: Number of audio channels
    /// - `frame_count`: Number of frames to generate
    fn fill_buffer(&mut self, output: &mut [f32], sample_rate: f32, channels: usize, frame_count: usize);
    
    /// Check if this audio source is still active/playing
    fn is_active(&self) -> bool;
    
    /// Reset the audio source to its initial state
    fn reset(&mut self);
}

/// A simple test audio source that generates silence
/// This will be replaced by your DSP implementations later
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

// Cached audio configuration to avoid repeated queries
#[derive(Clone)]
struct AudioConfig {
    channels: u16,
    sample_rate: f32,
}

pub struct VoiceRenderer {
    stream: cpal::Stream,
    sample_clock: Arc<AtomicU64>,
    config: AudioConfig,
    audio_source: Arc<std::sync::Mutex<Box<dyn AudioSource>>>,
}

impl VoiceRenderer {
    pub fn new() -> Self {
        Self::with_audio_source(Box::new(SilenceSource))
    }

    pub fn with_audio_source(audio_source: Box<dyn AudioSource>) -> Self {
        Self::with_config_and_source(48000.0, None, audio_source)
    }

    pub fn with_sample_rate(desired_sample_rate: f32) -> Self {
        Self::with_config_and_source(desired_sample_rate, None, Box::new(SilenceSource))
    }
    
    pub fn with_config_and_source(
        desired_sample_rate: f32, 
        buffer_frames: Option<u32>,
        audio_source: Box<dyn AudioSource>
    ) -> Self {
        
        let host = Self::get_preferred_host();
        let device = host.default_output_device().expect("No default output device");
        
        let default_config = device.default_output_config().expect("No default output config");
        
        // Check if our desired sample rate is supported
        let mut actual_sample_rate = desired_sample_rate as u32;
        let mut config_found = false;
        
        if let Ok(supported_configs) = device.supported_output_configs() {
            for supported_config in supported_configs {
                // Check if our desired rate is in this range
                if desired_sample_rate as u32 >= supported_config.min_sample_rate().0 
                   && desired_sample_rate as u32 <= supported_config.max_sample_rate().0 {
                    actual_sample_rate = desired_sample_rate as u32;
                    config_found = true;
                    break;
                }
            }
        }
        
        // If desired sample rate isn't supported, fall back to device default
        if !config_found {
            actual_sample_rate = default_config.sample_rate().0;
        }
        
        let config = cpal::StreamConfig {
            channels: default_config.channels(),
            sample_rate: cpal::SampleRate(actual_sample_rate),
            buffer_size: if let Some(frames) = buffer_frames {
                cpal::BufferSize::Fixed(frames)
            } else {
                cpal::BufferSize::Default
            },
        };

        let audio_config = AudioConfig {
            channels: config.channels,
            sample_rate: config.sample_rate.0 as f32,
        };

        let sample_clock = Arc::new(AtomicU64::new(0));
        let sc_clone = Arc::clone(&sample_clock);
        
        let audio_source = Arc::new(std::sync::Mutex::new(audio_source));
        let source_clone = Arc::clone(&audio_source);

        let stream = match default_config.sample_format() {
            cpal::SampleFormat::F32 => Self::build_stream::<f32>(&device, &config, sc_clone, audio_config.clone(), source_clone),
            cpal::SampleFormat::I16 => Self::build_stream::<i16>(&device, &config, sc_clone, audio_config.clone(), source_clone),
            cpal::SampleFormat::U16 => Self::build_stream::<u16>(&device, &config, sc_clone, audio_config.clone(), source_clone),
            _ => panic!("Unsupported sample format"),
        };

        Self { 
            stream, 
            sample_clock, 
            config: audio_config,
            audio_source,
        }
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        sample_clock: Arc<AtomicU64>,
        audio_config: AudioConfig,
        audio_source: Arc<std::sync::Mutex<Box<dyn AudioSource>>>,
    ) -> cpal::Stream
    where
        T: SizedSample + FromSample<f32> + Copy,
    {
        let channels = audio_config.channels as usize;
        
        device.build_output_stream(
            config,
            move |data: &mut [T], _info: &cpal::OutputCallbackInfo| {
                let frame_count = data.len() / channels;
                
                // Update sample count
                sample_clock.fetch_add(frame_count as u64, Ordering::Relaxed);
                
                // Create temporary f32 buffer for processing
                let mut float_buffer = vec![0.0f32; data.len()];
                
                // Get audio samples from the source
                if let Ok(mut source) = audio_source.lock() {
                    if source.is_active() {
                        source.fill_buffer(&mut float_buffer, audio_config.sample_rate, channels, frame_count);
                    }
                    // If source is inactive, buffer remains zeros (silence)
                }
                
                // Convert f32 samples to target format
                for (i, &sample) in float_buffer.iter().enumerate() {
                    data[i] = T::from_sample(sample);
                }
            },
            move |err| { eprintln!("Stream error: {:?}", err); },
            None,
        ).expect("Failed to build output stream")
    }

    pub fn play(&self) {
        self.stream.play().expect("Failed to play stream");
    }

    pub fn sample_rate(&self) -> f32 {
        self.config.sample_rate
    }

    pub fn get_sample_count(&self) -> u64 {
        self.sample_clock.load(Ordering::Relaxed)
    }

    pub fn get_playback_time(&self) -> f32 {
        self.sample_clock.load(Ordering::Relaxed) as f32 / self.config.sample_rate
    }
    
    // Cached configuration access
    pub fn channels(&self) -> u16 {
        self.config.channels
    }

    /// Replace the current audio source with a new one
    pub fn set_audio_source(&self, new_source: Box<dyn AudioSource>) {
        if let Ok(mut source) = self.audio_source.lock() {
            *source = new_source;
        }
    }
        
    /// Get a reference to the current audio source for configuration
    pub fn with_audio_source_mut<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut Box<dyn AudioSource>) -> R,
    {
        self.audio_source.lock().ok().map(|mut source| f(&mut source))
    }

    fn get_preferred_host() -> cpal::Host {
        #[cfg(target_os = "windows")]
        {
            // Try ASIO first, fall back to WASAPI
            if let Ok(host) = cpal::host_from_id(cpal::HostId::Asio) {
                return host;
            }
        }
        
        #[cfg(target_os = "linux")]
        {
            // Try JACK first, fall back to ALSA
            if let Ok(host) = cpal::host_from_id(cpal::HostId::Jack) {
                return host;
            }
        }
        
        // Default host for other platforms or fallback
        cpal::default_host()
    }
}

impl Default for VoiceRenderer {
    fn default() -> Self {
        Self::new()
    }
}