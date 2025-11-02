pub mod tables;
pub mod oscillators;
pub mod envelopes;
pub mod noise;

use crate::rt_processing::routing::AudioSource as RoutingAudioSource;

pub struct WaveformAdapter<T: crate::rt_processing::voice_renderer::AudioSource> {
    source: T,
    temp_buffer: Vec<f32>,
}

impl<T: crate::rt_processing::voice_renderer::AudioSource> RoutingAudioSource for WaveformAdapter<T> {
    fn render(&mut self, output: &mut [&mut [f32]], frames: usize, sample_rate: f32) {
        let channels = output.len();
        
        // Resize temp buffer if needed
        let needed_size = frames * channels;
        if self.temp_buffer.len() < needed_size {
            self.temp_buffer.resize(needed_size, 0.0);
        }
        
        // Fill interleaved temp buffer
        self.source.fill_buffer(&mut self.temp_buffer[..needed_size], sample_rate, channels, frames);
        
        // De-interleave into output
        for frame in 0..frames {
            for ch in 0..channels {
                output[ch][frame] = self.temp_buffer[frame * channels + ch];
            }
        }
    }
}