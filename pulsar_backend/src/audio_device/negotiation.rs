use crate::audio_device::enumeration::DeviceInfo;
use cpal::{SampleFormat, SampleRate, StreamConfig, BufferSize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRatePriority {
    HighestQuality,
    LowestLatency,
    Standard,
    Exact, 
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelPriority {
    Maximum,
    Minimum,
    Default,
    Exact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferSizePriority {
    MinimumLatency,
    MaximumThroughput,
    Balanced,
    Default,
    Exact,
}

#[derive(Debug, Clone)]
pub struct ConfigurationRequest {
    pub sample_rate: Option<u32>,
    pub sample_rate_priority: SampleRatePriority,
    
    pub channels: Option<u16>,
    pub channel_priority: ChannelPriority,
    
    pub buffer_size: Option<u32>,
    pub buffer_size_priority: BufferSizePriority,
    
    pub sample_format: Option<SampleFormat>,
    pub allow_format_conversion: bool,
}

impl ConfigurationRequest {
    pub fn new() -> Self {
        Self {
            sample_rate: None,
            sample_rate_priority: SampleRatePriority::Standard,
            channels: None,
            channel_priority: ChannelPriority::Default,
            buffer_size: None,
            buffer_size_priority: BufferSizePriority::Balanced,
            sample_format: None,
            allow_format_conversion: true,
        }
    }

    pub fn with_sample_rate(mut self, rate:u32) -> Self {
        self.sample_rate = Some(rate);
        self
    }

    pub fn with_sample_rate_priority(mut self, priority: SampleRatePriority) -> Self {
        self.sample_rate_priority = priority;
        self
    }
    
    pub fn with_channels(mut self, channels: u16) -> Self {
        self.channels = Some(channels);
        self
    }
    
    pub fn with_channel_priority(mut self, priority: ChannelPriority) -> Self {
        self.channel_priority = priority;
        self
    }
    
    pub fn with_buffer_size(mut self, size: u32) -> Self {
        self.buffer_size = Some(size);
        self
    }
    
    pub fn with_buffer_size_priority(mut self, priority: BufferSizePriority) -> Self {
        self.buffer_size_priority = priority;
        self
    }
    
    pub fn with_sample_format(mut self, format: SampleFormat) -> Self {
        self.sample_format = Some(format);
        self
    }
    
    pub fn allow_format_conversion(mut self, allow: bool) -> Self {
        self.allow_format_conversion = allow;
        self
    }
    
    pub fn low_latency() -> Self {
        Self::new()
            .with_sample_rate(48000)
            .with_buffer_size(128)
            .with_buffer_size_priority(BufferSizePriority::MinimumLatency)
            .with_sample_rate_priority(SampleRatePriority::Standard)
    }
    
    pub fn high_quality() -> Self {
        Self::new()
            .with_sample_rate(96000)
            .with_buffer_size(512)
            .with_sample_rate_priority(SampleRatePriority::HighestQuality)
            .with_buffer_size_priority(BufferSizePriority::Balanced)
    }
    
    pub fn balanced() -> Self {
        Self::new()
            .with_sample_rate(48000)
            .with_buffer_size(256)
            .with_sample_rate_priority(SampleRatePriority::Standard)
            .with_buffer_size_priority(BufferSizePriority::Balanced)
    }
    
    pub fn music_production() -> Self {
        Self::new()
            .with_sample_rate(44100)
            .with_buffer_size(128)
            .with_sample_rate_priority(SampleRatePriority::Exact)
            .with_buffer_size_priority(BufferSizePriority::MinimumLatency)
    }
}

impl Default for ConfigurationRequest {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct NegotiatedConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_size: BufferSize,
    pub sample_format: SampleFormat,
    pub stream_config: StreamConfig,
    
    pub sample_rate_matched: bool,
    pub channels_matched: bool,
    pub buffer_size_matched: bool,
    pub format_matched: bool,
}

impl fmt::Display for NegotiatedConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}ch @ {}Hz, buffer: {:?}, format: {:?}",
            self.channels,
            self.sample_rate,
            self.buffer_size,
            self.sample_format
        )
    }
}

#[derive(Debug, Clone)]
pub enum NegotiationError {
    SampleRateNotSupported { requested: u32, available: Vec<u32> },
    ChannelsNotSupported { requested: u16, available: Vec<u16> },
    FormatNotSupported { requested: SampleFormat, available: Vec<SampleFormat> },
    BufferSizeNotSupported { requested: u32 },
    NoCompatibleConfiguration,
    DeviceQueryFailed(String),
}

impl fmt::Display for NegotiationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SampleRateNotSupported { requested, available } => {
                write!(f, "Sample rate {} not supported. Available: {:?}", requested, available)
            }
            Self::ChannelsNotSupported { requested, available } => {
                write!(f, "Channel count {} not supported. Available: {:?}", requested, available)
            }
            Self::FormatNotSupported { requested, available } => {
                write!(f, "Sample format {:?} not supported. Available: {:?}", requested, available)
            }
            Self::BufferSizeNotSupported { requested } => {
                write!(f, "Buffer size {} not supported by device", requested)
            }
            Self::NoCompatibleConfiguration => {
                write!(f, "No compatible configuration found for device")
            }
            Self::DeviceQueryFailed(msg) => {
                write!(f, "Device query failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for NegotiationError {}

pub type NegotiationResult<T> = Result<T, NegotiationError>;

pub struct ConfigNegotiator;
impl ConfigNegotiator {
    pub fn negotiate(
        device_info: &DeviceInfo,
        request: &ConfigurationRequest,
    ) -> NegotiationResult<NegotiatedConfig> {
        let sample_rate = Self::negotiate_sample_rate(device_info, request)?;
        let channels = Self::negotiate_channels(device_info, request)?;
        let sample_format = Self::negotiate_sample_format(device_info, request)?;
        let buffer_size = Self::negotiate_buffer_size(request);
        
        let sample_rate_matched = request.sample_rate.map_or(true, |r| r == sample_rate);
        let channels_matched = request.channels.map_or(true, |r| r == channels);
        let format_matched = request.sample_format.map_or(true, |r| r == sample_format);
        let buffer_size_matched = match (&request.buffer_size, &buffer_size) {
            (Some(req), BufferSize::Fixed(actual)) => *req == *actual,
            (None, _) => true,
            _ => false,
        };
        
        let stream_config = StreamConfig {
            channels,
            sample_rate: SampleRate(sample_rate),
            buffer_size: buffer_size.clone(),
        };
        
        Ok(NegotiatedConfig {
            sample_rate,
            channels,
            buffer_size,
            sample_format,
            stream_config,
            sample_rate_matched,
            channels_matched,
            buffer_size_matched,
            format_matched,
        })
    }
    
    fn negotiate_sample_rate(
        device_info: &DeviceInfo,
        request: &ConfigurationRequest,
    ) -> NegotiationResult<u32> {
        if let Some(requested) = request.sample_rate {
            if request.sample_rate_priority == SampleRatePriority::Exact {
                if Self::is_sample_rate_supported(device_info, requested) {
                    return Ok(requested);
                } else {
                    return Err(NegotiationError::SampleRateNotSupported {
                        requested,
                        available: device_info.supported_sample_rates.clone(),
                    });
                }
            }
            
            if Self::is_sample_rate_supported(device_info, requested) {
                return Ok(requested);
            }
        }
        
        match request.sample_rate_priority {
            SampleRatePriority::HighestQuality => {
                device_info.supported_sample_rates
                    .iter()
                    .max()
                    .copied()
                    .or(Some(device_info.max_sample_rate))
                    .ok_or(NegotiationError::NoCompatibleConfiguration)
            }
            SampleRatePriority::LowestLatency => {
                Self::find_best_standard_rate(device_info)
                    .or_else(|| device_info.supported_sample_rates.iter().min().copied())
                    .or(Some(device_info.min_sample_rate))
                    .ok_or(NegotiationError::NoCompatibleConfiguration)
            }
            SampleRatePriority::Standard => {
                Self::find_best_standard_rate(device_info)
                    .or(Some(device_info.default_sample_rate))
                    .ok_or(NegotiationError::NoCompatibleConfiguration)
            }
            SampleRatePriority::Exact => {
                Ok(device_info.default_sample_rate)
            }
        }
    }
    
    fn find_best_standard_rate(device_info: &DeviceInfo) -> Option<u32> {
        for &rate in &[48000, 44100, 96000, 88200] {
            if Self::is_sample_rate_supported(device_info, rate) {
                return Some(rate);
            }
        }
        None
    }
    
    fn is_sample_rate_supported(device_info: &DeviceInfo, rate: u32) -> bool {
        rate >= device_info.min_sample_rate 
            && rate <= device_info.max_sample_rate
    }
    
    fn negotiate_channels(
        device_info: &DeviceInfo,
        request: &ConfigurationRequest,
    ) -> NegotiationResult<u16> {
        if let Some(requested) = request.channels {
            if request.channel_priority == ChannelPriority::Exact {
                if device_info.supported_channels.contains(&requested) 
                    || requested <= device_info.max_channels {
                    return Ok(requested);
                } else {
                    return Err(NegotiationError::ChannelsNotSupported {
                        requested,
                        available: device_info.supported_channels.clone(),
                    });
                }
            }
            
            if device_info.supported_channels.contains(&requested) 
                || requested <= device_info.max_channels {
                return Ok(requested);
            }
        }
        
        match request.channel_priority {
            ChannelPriority::Maximum => {
                Ok(device_info.max_channels)
            }
            ChannelPriority::Minimum => {
                Ok(device_info.supported_channels
                    .iter()
                    .min()
                    .copied()
                    .unwrap_or(device_info.default_channels))
            }
            ChannelPriority::Default => {
                Ok(device_info.default_channels)
            }
            ChannelPriority::Exact => {
                Ok(device_info.default_channels)
            }
        }
    }
    
    fn negotiate_sample_format(
        device_info: &DeviceInfo,
        request: &ConfigurationRequest,
    ) -> NegotiationResult<SampleFormat> {
        if let Some(requested_format) = request.sample_format {
            if device_info.supported_sample_formats.contains(&requested_format) {
                return Ok(requested_format);
            }
            
            if !request.allow_format_conversion {
                return Err(NegotiationError::FormatNotSupported {
                    requested: requested_format,
                    available: device_info.supported_sample_formats.clone(),
                });
            }
        }
        
        if device_info.supported_sample_formats.contains(&device_info.default_sample_format) {
            return Ok(device_info.default_sample_format);
        }
        
        for &format in &[SampleFormat::F32, SampleFormat::I16, SampleFormat::U16] {
            if device_info.supported_sample_formats.contains(&format) {
                return Ok(format);
            }
        }
        
        device_info.supported_sample_formats
            .first()
            .copied()
            .ok_or(NegotiationError::NoCompatibleConfiguration)
    }
    
    fn negotiate_buffer_size(request: &ConfigurationRequest) -> BufferSize {
        if let Some(requested_size) = request.buffer_size {
            match request.buffer_size_priority {
                BufferSizePriority::Exact => BufferSize::Fixed(requested_size),
                _ => {
                    BufferSize::Fixed(requested_size)
                }
            }
        } else {
            match request.buffer_size_priority {
                BufferSizePriority::MinimumLatency => BufferSize::Fixed(128),
                BufferSizePriority::MaximumThroughput => BufferSize::Fixed(2048),
                BufferSizePriority::Balanced => BufferSize::Fixed(512),
                BufferSizePriority::Default => BufferSize::Default,
                BufferSizePriority::Exact => BufferSize::Default,
            }
        }
    }
    
    pub fn calculate_latency_ms(sample_rate: u32, buffer_size: u32) -> f32 {
        (buffer_size as f32 / sample_rate as f32) * 1000.0
    }
    
    pub fn find_closest_sample_rate(device_info: &DeviceInfo, target: u32) -> Option<u32> {
        if device_info.supported_sample_rates.is_empty() {
            if target >= device_info.min_sample_rate && target <= device_info.max_sample_rate {
                return Some(target);
            }
            if target < device_info.min_sample_rate {
                return Some(device_info.min_sample_rate);
            }
            return Some(device_info.max_sample_rate);
        }
        
        device_info.supported_sample_rates
            .iter()
            .min_by_key(|&&rate| {
                let diff = (rate as i64 - target as i64).abs();
                diff
            })
            .copied()
    }
    
    pub fn validate_config(
        device_info: &DeviceInfo,
        sample_rate: u32,
        channels: u16,
        format: SampleFormat,
    ) -> Result<(), NegotiationError> {
        if !Self::is_sample_rate_supported(device_info, sample_rate) {
            return Err(NegotiationError::SampleRateNotSupported {
                requested: sample_rate,
                available: device_info.supported_sample_rates.clone(),
            });
        }
        
        if channels > device_info.max_channels {
            return Err(NegotiationError::ChannelsNotSupported {
                requested: channels,
                available: device_info.supported_channels.clone(),
            });
        }
        
        if !device_info.supported_sample_formats.contains(&format) {
            return Err(NegotiationError::FormatNotSupported {
                requested: format,
                available: device_info.supported_sample_formats.clone(),
            });
        }
        
        Ok(())
    }
}