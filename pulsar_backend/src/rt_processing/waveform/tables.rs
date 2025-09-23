use std::f32::consts::PI;
use std::sync::OnceLock;

// Optimized sine table configuration
const SINE_TABLE_SIZE: usize = 8192; // Power of 2 for fast masking
const SINE_TABLE_MASK: usize = SINE_TABLE_SIZE - 1;

// Static lookup tables - initialized once, used everywhere
static SINE_TABLE: OnceLock<Vec<f32>> = OnceLock::new();
static TRIANGLE_TABLE: OnceLock<Vec<f32>> = OnceLock::new();
static SAWTOOTH_TABLE: OnceLock<Vec<f32>> = OnceLock::new();
static SQUARE_TABLE: OnceLock<Vec<f32>> = OnceLock::new();

/// Initialize all waveform tables
pub fn init_tables() {
    let _ = get_sine_table();
    let _ = get_triangle_table();
    let _ = get_sawtooth_table();
    let _ = get_square_table();
}

/// Get reference to the sine wave lookup table
pub fn get_sine_table() -> &'static [f32] {
    SINE_TABLE.get_or_init(|| {
        (0..SINE_TABLE_SIZE)
            .map(|i| (2.0 * PI * i as f32 / SINE_TABLE_SIZE as f32).sin())
            .collect()
    })
}

/// Get reference to the triangle wave lookup table
pub fn get_triangle_table() -> &'static [f32] {
    TRIANGLE_TABLE.get_or_init(|| {
        (0..SINE_TABLE_SIZE)
            .map(|i| {
                let phase = i as f32 / SINE_TABLE_SIZE as f32;
                if phase < 0.25 {
                    4.0 * phase
                } else if phase < 0.75 {
                    2.0 - 4.0 * phase
                } else {
                    4.0 * phase - 4.0
                }
            })
            .collect()
    })
}

/// Get reference to the sawtooth wave lookup table
pub fn get_sawtooth_table() -> &'static [f32] {
    SAWTOOTH_TABLE.get_or_init(|| {
        (0..SINE_TABLE_SIZE)
            .map(|i| {
                let phase = i as f32 / SINE_TABLE_SIZE as f32;
                2.0 * phase - 1.0
            })
            .collect()
    })
}

/// Get reference to the square wave lookup table
pub fn get_square_table() -> &'static [f32] {
    SQUARE_TABLE.get_or_init(|| {
        (0..SINE_TABLE_SIZE)
            .map(|i| {
                let phase = i as f32 / SINE_TABLE_SIZE as f32;
                if phase < 0.5 { 1.0 } else { -1.0 }
            })
            .collect()
    })
}

/// High-quality interpolated table lookup for sine waves
#[inline]
pub fn interpolated_sine(phase: f32) -> f32 {
    interpolated_lookup(get_sine_table(), phase)
}

/// High-quality interpolated table lookup for triangle waves
#[inline]
pub fn interpolated_triangle(phase: f32) -> f32 {
    interpolated_lookup(get_triangle_table(), phase)
}

/// High-quality interpolated table lookup for sawtooth waves
#[inline]
pub fn interpolated_sawtooth(phase: f32) -> f32 {
    interpolated_lookup(get_sawtooth_table(), phase)
}

/// High-quality interpolated table lookup for square waves
#[inline]
pub fn interpolated_square(phase: f32) -> f32 {
    interpolated_lookup(get_square_table(), phase)
}

/// Generic interpolated table lookup function
/// Phase should be normalized to [0.0, 1.0)
#[inline]
pub fn interpolated_lookup(table: &[f32], phase: f32) -> f32 {
    let scaled_phase = phase * SINE_TABLE_SIZE as f32;
    let index = scaled_phase as usize & SINE_TABLE_MASK;
    let frac = scaled_phase - (scaled_phase as usize as f32);
    
    let sample1 = table[index];
    let sample2 = table[(index + 1) & SINE_TABLE_MASK];
    
    // Linear interpolation for smooth transitions
    sample1 + frac * (sample2 - sample1)
}

/// Fast, non-interpolated table lookup (for when performance is critical)
#[inline]
pub fn fast_sine(phase: f32) -> f32 {
    fast_lookup(get_sine_table(), phase)
}

/// Fast, non-interpolated table lookup (for when performance is critical)
#[inline]
pub fn fast_triangle(phase: f32) -> f32 {
    fast_lookup(get_triangle_table(), phase)
}

/// Fast, non-interpolated table lookup (for when performance is critical)
#[inline]
pub fn fast_sawtooth(phase: f32) -> f32 {
    fast_lookup(get_sawtooth_table(), phase)
}

/// Fast, non-interpolated table lookup (for when performance is critical)
#[inline]
pub fn fast_square(phase: f32) -> f32 {
    fast_lookup(get_square_table(), phase)
}

/// Generic fast (non-interpolated) table lookup
#[inline]
pub fn fast_lookup(table: &[f32], phase: f32) -> f32 {
    let index = (phase * SINE_TABLE_SIZE as f32) as usize & SINE_TABLE_MASK;
    table[index]
}

/// Normalize phase to [0.0, 1.0) range to prevent accumulation errors
#[inline]
pub fn normalize_phase(phase: f32) -> f32 {
    phase - phase.floor()
}

/// Phase increment calculation helper
#[inline]
pub fn phase_increment(frequency: f32, sample_rate: f32) -> f32 {
    frequency / sample_rate
}

/// Waveform type enumeration for dynamic waveform selection
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WaveformType {
    Sine,
    Triangle,
    Sawtooth,
    Square,
}

impl WaveformType {
    /// Get interpolated sample for this waveform type
    pub fn interpolated_sample(self, phase: f32) -> f32 {
        match self {
            WaveformType::Sine => interpolated_sine(phase),
            WaveformType::Triangle => interpolated_triangle(phase),
            WaveformType::Sawtooth => interpolated_sawtooth(phase),
            WaveformType::Square => interpolated_square(phase),
        }
    }
    
    /// Get fast (non-interpolated) sample for this waveform type
    pub fn fast_sample(self, phase: f32) -> f32 {
        match self {
            WaveformType::Sine => fast_sine(phase),
            WaveformType::Triangle => fast_triangle(phase),
            WaveformType::Sawtooth => fast_sawtooth(phase),
            WaveformType::Square => fast_square(phase),
        }
    }
    
    /// Get the lookup table for this waveform type
    pub fn table(self) -> &'static [f32] {
        match self {
            WaveformType::Sine => get_sine_table(),
            WaveformType::Triangle => get_triangle_table(),
            WaveformType::Sawtooth => get_sawtooth_table(),
            WaveformType::Square => get_square_table(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_table_initialization() {
        // All tables should initialize without panic
        let sine = get_sine_table();
        let triangle = get_triangle_table();
        let sawtooth = get_sawtooth_table();
        let square = get_square_table();
        
        assert_eq!(sine.len(), SINE_TABLE_SIZE);
        assert_eq!(triangle.len(), SINE_TABLE_SIZE);
        assert_eq!(sawtooth.len(), SINE_TABLE_SIZE);
        assert_eq!(square.len(), SINE_TABLE_SIZE);
    }
    
    #[test]
    fn test_sine_wave_properties() {
        // Test that sine wave has expected properties
        assert!((interpolated_sine(0.0) - 0.0).abs() < 0.001);
        assert!((interpolated_sine(0.25) - 1.0).abs() < 0.001);
        assert!((interpolated_sine(0.5) - 0.0).abs() < 0.001);
        assert!((interpolated_sine(0.75) - (-1.0)).abs() < 0.001);
    }
    
    #[test]
    fn test_triangle_wave_properties() {
        // Test triangle wave properties
        assert!((interpolated_triangle(0.0) - 0.0).abs() < 0.001);
        assert!((interpolated_triangle(0.25) - 1.0).abs() < 0.001);
        assert!((interpolated_triangle(0.5) - 0.0).abs() < 0.001);
        assert!((interpolated_triangle(0.75) - (-1.0)).abs() < 0.001);
    }
    
    #[test]
    fn test_phase_normalization() {
        assert_eq!(normalize_phase(1.5), 0.5);
        assert_eq!(normalize_phase(2.0), 0.0);
        assert_eq!(normalize_phase(-0.5), 0.5);
    }
    
    #[test]
    fn test_waveform_type_enum() {
        let phase = 0.25;
        
        let sine_sample = WaveformType::Sine.interpolated_sample(phase);
        let triangle_sample = WaveformType::Triangle.interpolated_sample(phase);
        
        assert!((sine_sample - 1.0).abs() < 0.001);
        assert!((triangle_sample - 1.0).abs() < 0.001);
    }
}