use std::sync::Arc;
use spin::RwLock;

use crate::rt_processing::performance::PerformanceMonitor;

/// Trait for any renderable audio source.
/// Non-interleaved, [channel][frame]
pub trait AudioSource: Send + Sync {
    fn render(&mut self, output: &mut [&mut [f32]], frames: usize, sample_rate: f32);
}

/// Pan law
#[derive(Copy, Clone, Debug)]
pub enum PanLaw {
    Linear,
    EqualPower,
}

/// Pan position (-1.0 = left, 0.0 = center, 1.0 = right)
#[derive(Copy, Clone, Debug)]
pub struct Pan {
    pub value: f32,
    pub law: PanLaw,
}

impl Pan {
    #[inline(always)]
    pub fn gains(&self) -> (f32, f32) {
        match self.law {
            PanLaw::Linear => {
                let l = 0.5 * (1.0 - self.value);
                let r = 0.5 * (1.0 + self.value);
                (l, r)
            }
            PanLaw::EqualPower => {
                let theta = (self.value + 1.0) * std::f32::consts::FRAC_PI_4;
                (theta.cos(), theta.sin())
            }
        }
    }
}

/// Represents a routed audio source.
/// Note: we store a 'static trait object so it's straightforward to push
/// Boxed adapters created from local types.
pub struct RoutedSource {
    pub source: Box<dyn AudioSource + 'static>,
    pub gain: f32,
    pub pan: Pan,
    pub bus: usize, // 0 = master, >0 = aux bus
}

/// The main router/mixer
pub struct Router {
    sources: Arc<RwLock<Vec<RoutedSource>>>,
    channels: usize,
    sample_rate: f32,
    // Scratch buffer: [channels][frames]
    scratch: Vec<Vec<f32>>,
    num_buses: usize,
}

impl Router {
    pub fn new(channels: usize, sample_rate: f32, num_buses: usize, max_frames: usize) -> Self {
        let mut scratch = Vec::with_capacity(channels);
        for _ in 0..channels {
            scratch.push(vec![0.0; max_frames]);
        }

        Self {
            sources: Arc::new(RwLock::new(Vec::new())),
            channels,
            sample_rate,
            scratch,
            num_buses: num_buses.max(1),
        }
    }

    /// Accept a 'static boxed routing AudioSource.
    /// We take &self because we mutate the internal RwLock, not `self` itself.
    pub fn add_source(&self, source: Box<dyn AudioSource + 'static>, gain: f32, pan: Pan, bus: usize) {
        let mut guard = self.sources.write();
        guard.push(RoutedSource { source, gain, pan, bus });
    }

    pub fn clear_sources(&self) {
        self.sources.write().clear();
    }

    /// Process all sources → mix into interleaved output buffer
    pub fn process(&mut self, output: &mut [f32], perf_monitor: Option<&PerformanceMonitor>) {
        let frames = output.len() / self.channels;

        // zero master scratch
        for ch in 0..self.channels {
            self.scratch[ch][..frames].fill(0.0);
        }

        // allocate + zero bus buffers: [bus][channel][frame]
        let mut bus_buffers: Vec<Vec<Vec<f32>>> =
            (0..self.num_buses)
                .map(|_| (0..self.channels).map(|_| vec![0.0; frames]).collect())
                .collect();

        // mix all sources into their assigned bus
        let mut guard = self.sources.write();
        for routed in guard.iter_mut() {
            // temporary buffer for this source [channel][frame]
            let mut temp: Vec<Vec<f32>> = (0..self.channels)
                .map(|_| vec![0.0; frames])
                .collect();

            let mut views: Vec<&mut [f32]> =
                temp.iter_mut().map(|c| &mut c[..]).collect();

            routed.source.render(&mut views, frames, self.sample_rate);

            let bus = routed.bus.min(self.num_buses - 1);

            if self.channels == 2 {
                // stereo panning for mono → stereo
                let (lg, rg) = routed.pan.gains();
                for i in 0..frames {
                    // assume source filled views[0] as mono
                    let s = views[0][i] * routed.gain;
                    bus_buffers[bus][0][i] += s * lg;
                    bus_buffers[bus][1][i] += s * rg;
                }
            } else {
                // generic n-channel, apply gain only
                for ch in 0..self.channels {
                    for i in 0..frames {
                        bus_buffers[bus][ch][i] += views[ch][i] * routed.gain;
                    }
                }
            }
        }

        // finally mix all buses into master (bus 0 is master)
        for bus in 0..self.num_buses {
            for ch in 0..self.channels {
                for i in 0..frames {
                    self.scratch[ch][i] += bus_buffers[bus][ch][i];
                }
            }
        }

        // write interleaved
        for i in 0..frames {
            for ch in 0..self.channels {
                output[i * self.channels + ch] = self.scratch[ch][i];
            }
        }

        let _guard = perf_monitor.map(|p| p.scoped_callback());

        if let Some(monitor) = perf_monitor {
            monitor.add_frames_processed(frames as u64);
        }
    }
}
