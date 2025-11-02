use cpal::traits::{DeviceTrait, HostTrait};
use std::{fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInfo {
    pub id: cpal::HostId,
    pub name: String,
    pub is_available: bool,
    pub is_default: bool,
}

impl fmt::Display for HostInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, if self.is_default { "default" } else { "available" })
    }
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub host_id: cpal::HostId,
    pub is_default: bool,
    pub is_input: bool,
    pub is_output: bool,
    
    pub supported_sample_rates: Vec<u32>,
    pub min_sample_rate: u32,
    pub max_sample_rate: u32,
    pub default_sample_rate: u32,
    
    pub supported_channels: Vec<u16>,
    pub max_channels: u16,
    pub default_channels: u16,
    
    pub supported_sample_formats: Vec<cpal::SampleFormat>,
    pub default_sample_format: cpal::SampleFormat,
    
    pub(crate) device_index: usize,
}

impl fmt::Display for DeviceInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}ch @ {}Hz{}]",
            self.name,
            self.default_channels,
            self.default_sample_rate,
            if self.is_default { " (default)" } else { "" }
        )
    }
}

pub type EnumResult<T> = Result<T, EnumError>;

#[derive(Debug)]
pub enum EnumError {
    NoDevicesFound,
    DeviceNotFound(String),
    HostNotAvailable(String),
    QueryFailed(String),
    InvalidDeviceIndex(usize),
}

impl fmt::Display for EnumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDevicesFound => write!(f, "No audio devices found"),
            Self::DeviceNotFound(name) => write!(f, "Device not found: {}", name),
            Self::HostNotAvailable(host) => write!(f, "Audio host not available: {}", host),
            Self::QueryFailed(msg) => write!(f, "Device query failed: {}", msg),
            Self::InvalidDeviceIndex(idx) => write!(f, "Invalid device index: {}", idx),
        }
    }
}

impl std::error::Error for EnumError {}

pub struct DeviceEnumerator {
    hosts: Vec<HostInfo>,
    devices: Vec<(cpal::Device, DeviceInfo)>,
}

impl DeviceEnumerator {
    pub fn new() -> EnumResult<Self> {
        let hosts = Self::enumerate_hosts();
        let devices = Self::scan_all_devices(&hosts)?;

        Ok(Self {
            hosts,
            devices,
        })
    }

    pub fn enumerate_hosts() -> Vec<HostInfo> {
        let default_host_id = cpal::default_host().id();
        let mut hosts = Vec::new();

        let host_ids = [
            #[cfg(target_os = "windows")]
            cpal::HostId::Wasapi,
            #[cfg(target_os = "windows")]
            cpal::HostId::Asio,
            
            #[cfg(target_os = "linux")]
            cpal::HostId::Alsa,
            #[cfg(target_os = "linux")]
            cpal::HostId::Jack,
        ];

        for &host_id in &host_ids {
            let is_available = cpal::host_from_id(host_id).is_ok();
            let is_default = host_id == default_host_id;
            
            hosts.push(HostInfo {
                id: host_id,
                name: Self::host_id_name(host_id),
                is_available,
                is_default,
            });
        }

        hosts
    }

    fn host_id_name(id: cpal::HostId) -> String {
        match id {
            cpal::HostId::Alsa => "ALSA".to_string(),
            cpal::HostId::Jack => "JACK".to_string(),
        }
    }

    fn scan_all_devices(hosts: &[HostInfo]) -> EnumResult<Vec<(cpal::Device, DeviceInfo)>> {
        let mut all_devices = Vec::new();
        let mut device_index = 0;
        
        for host_info in hosts {
            if !host_info.is_available {
                continue;
            }
            
            let host = match cpal::host_from_id(host_info.id) {
                Ok(h) => h,
                Err(_) => continue,
            };
            
            // Get default devices for this host
            let default_output = host.default_output_device();
            let default_input = host.default_input_device();
            
            // Enumerate output devices
            if let Ok(devices) = host.output_devices() {
                for device in devices {
                    let device_name = device.name().unwrap_or_else(|_| "Unknown Device".to_string());
                    let is_default = default_output
                        .as_ref()
                        .and_then(|d| d.name().ok())
                        .map(|name| name == device_name)
                        .unwrap_or(false);
                    
                    if let Ok(info) = Self::query_device_info(&device, host_info.id, is_default, false, true, device_index) {
                        all_devices.push((device, info));
                        device_index += 1;
                    }
                }
            }
            
            // Enumerate input devices
            if let Ok(devices) = host.input_devices() {
                for device in devices {
                    let device_name = device.name().unwrap_or_else(|_| "Unknown Device".to_string());
                    let is_default = default_input
                        .as_ref()
                        .and_then(|d| d.name().ok())
                        .map(|name| name == device_name)
                        .unwrap_or(false);
                    
                    if let Ok(info) = Self::query_device_info(&device, host_info.id, is_default, true, false, device_index) {
                        all_devices.push((device, info));
                        device_index += 1;
                    }
                }
            }
        }
        
        if all_devices.is_empty() {
            return Err(EnumError::NoDevicesFound);
        }
        
        Ok(all_devices)
    }

    fn query_device_info(
        device: &cpal::Device,
        host_id: cpal::HostId,
        is_default: bool,
        is_input: bool,
        is_output: bool,
        device_index: usize,
    ) -> EnumResult<DeviceInfo> {
        let name = device.name()
            .map_err(|e| EnumError::QueryFailed(format!("Failed to get device name: {}", e)))?;
        
        // Get default config
        let default_config = if is_output {
            device.default_output_config()
        } else {
            device.default_input_config()
        }.map_err(|e| EnumError::QueryFailed(format!("Failed to get default config: {}", e)))?;
        
        let default_sample_rate = default_config.sample_rate().0;
        let default_channels = default_config.channels();
        let default_sample_format = default_config.sample_format();
        
        // Enumerate supported configurations
        // We need to handle the two different iterator types separately
        let mut sample_rates = Vec::new();
        let mut min_sample_rate = u32::MAX;
        let mut max_sample_rate = 0u32;
        let mut channels_set = std::collections::HashSet::new();
        let mut max_channels = 0u16;
        let mut sample_formats = Vec::new();
        
        // Helper closure to process config ranges (works for both input and output)
        let mut process_config = |config_range: cpal::SupportedStreamConfigRange| {
            // Sample rates
            let min_sr = config_range.min_sample_rate().0;
            let max_sr = config_range.max_sample_rate().0;
            
            min_sample_rate = min_sample_rate.min(min_sr);
            max_sample_rate = max_sample_rate.max(max_sr);
            
            // Add common sample rates within this range
            for &rate in &[8000, 11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000] {
                if rate >= min_sr && rate <= max_sr {
                    sample_rates.push(rate);
                }
            }
            
            // Channels
            let channels = config_range.channels();
            channels_set.insert(channels);
            max_channels = max_channels.max(channels);
            
            // Sample formats
            let format = config_range.sample_format();
            if !sample_formats.contains(&format) {
                sample_formats.push(format);
            }
        };
        
        // Process configs based on device type
        if is_output {
            let configs = device.supported_output_configs()
                .map_err(|e| EnumError::QueryFailed(format!("Failed to get supported configs: {}", e)))?;
            for config_range in configs {
                process_config(config_range);
            }
        } else {
            let configs = device.supported_input_configs()
                .map_err(|e| EnumError::QueryFailed(format!("Failed to get supported configs: {}", e)))?;
            for config_range in configs {
                process_config(config_range);
            }
        }
        
        sample_rates.sort_unstable();
        sample_rates.dedup();
        
        let mut supported_channels: Vec<u16> = channels_set.into_iter().collect();
        supported_channels.sort_unstable();
        
        Ok(DeviceInfo {
            name,
            host_id,
            is_default,
            is_input,
            is_output,
            supported_sample_rates: sample_rates,
            min_sample_rate,
            max_sample_rate,
            default_sample_rate,
            supported_channels,
            max_channels,
            default_channels,
            supported_sample_formats: sample_formats,
            default_sample_format,
            device_index,
        })
    }

    pub fn available_hosts(&self) -> Vec<&HostInfo> {
        self.hosts.iter().filter(|h| h.is_available).collect()
    }
    
    /// Get all discovered devices
    pub fn all_devices(&self) -> Vec<&DeviceInfo> {
        self.devices.iter().map(|(_, info)| info).collect()
    }
    
    /// Get all output devices
    pub fn output_devices(&self) -> Vec<&DeviceInfo> {
        self.devices
            .iter()
            .map(|(_, info)| info)
            .filter(|info| info.is_output)
            .collect()
    }
    
    /// Get all input devices
    pub fn input_devices(&self) -> Vec<&DeviceInfo> {
        self.devices
            .iter()
            .map(|(_, info)| info)
            .filter(|info| info.is_input)
            .collect()
    }
    
    /// Get the default output device
    pub fn default_output_device(&self) -> EnumResult<&DeviceInfo> {
        self.devices
            .iter()
            .map(|(_, info)| info)
            .find(|info| info.is_output && info.is_default)
            .ok_or(EnumError::NoDevicesFound)
    }
    
    /// Get the default input device
    pub fn default_input_device(&self) -> EnumResult<&DeviceInfo> {
        self.devices
            .iter()
            .map(|(_, info)| info)
            .find(|info| info.is_input && info.is_default)
            .ok_or(EnumError::NoDevicesFound)
    }
    
    /// Find a device by name (case-insensitive partial match)
    pub fn find_device_by_name(&self, name: &str) -> EnumResult<&DeviceInfo> {
        let name_lower = name.to_lowercase();
        
        // Try exact match first
        if let Some(info) = self.devices
            .iter()
            .map(|(_, info)| info)
            .find(|info| info.name.to_lowercase() == name_lower)
        {
            return Ok(info);
        }
        
        // Try partial match
        self.devices
            .iter()
            .map(|(_, info)| info)
            .find(|info| info.name.to_lowercase().contains(&name_lower))
            .ok_or_else(|| EnumError::DeviceNotFound(name.to_string()))
    }
    
    /// Get device by index
    pub fn device_by_index(&self, index: usize) -> EnumResult<&DeviceInfo> {
        self.devices
            .iter()
            .map(|(_, info)| info)
            .find(|info| info.device_index == index)
            .ok_or(EnumError::InvalidDeviceIndex(index))
    }
    
    /// Select a device and return the actual CPAL device handle
    pub fn select_device(&self, device_info: &DeviceInfo) -> EnumResult<&cpal::Device> {
        self.devices
            .iter()
            .find(|(_, info)| info.device_index == device_info.device_index)
            .map(|(device, _)| device)
            .ok_or(EnumError::InvalidDeviceIndex(device_info.device_index))
    }
    
    /// Get the preferred host based on platform and availability
    pub fn preferred_host(&self) -> &HostInfo {
        #[cfg(target_os = "windows")]
        {
            // Prefer ASIO on Windows if available
            if let Some(asio) = self.hosts.iter().find(|h| h.id == cpal::HostId::Asio && h.is_available) {
                return asio;
            }
        }
        
        #[cfg(target_os = "linux")]
        {
            // Prefer JACK on Linux if available
            if let Some(jack) = self.hosts.iter().find(|h| h.id == cpal::HostId::Jack && h.is_available) {
                return jack;
            }
        }
        
        // Fall back to default host
        self.hosts.iter().find(|h| h.is_default).unwrap()
    }
    
    /// Print a formatted list of all devices
    pub fn print_device_list(&self) {
        println!("Available Audio Hosts:");
        for host in self.available_hosts() {
            println!("  {}", host);
        }
        println!();
        
        println!("Output Devices:");
        for (idx, device) in self.output_devices().iter().enumerate() {
            println!("  [{}] {}", idx, device);
            println!("      Sample rates: {} - {} Hz", device.min_sample_rate, device.max_sample_rate);
            println!("      Channels: {} (max: {})", device.default_channels, device.max_channels);
        }
        println!();
        
        println!("Input Devices:");
        for (idx, device) in self.input_devices().iter().enumerate() {
            println!("  [{}] {}", idx, device);
            println!("      Sample rates: {} - {} Hz", device.min_sample_rate, device.max_sample_rate);
            println!("      Channels: {} (max: {})", device.default_channels, device.max_channels);
        }
    }
}

impl Default for DeviceEnumerator {
    fn default() -> Self {
        Self::new().expect("Failed to enumerate audio devices")
    }
}