use super::{HardwareInfo, Telemetry};

pub fn detect_hardware_info() -> HardwareInfo {
    HardwareInfo {
        laptop_model: "Nitro Series (Windows)".to_string(),
        cpu_model: "CPU".to_string(),
        gpu_model: "GPU".to_string(),
    }
}

pub fn apply_turbo() -> Result<(), String> {
    Err("Windows EC/WMI backend is not implemented yet.".to_string())
}

pub fn apply_auto() -> Result<(), String> {
    Err("Windows EC/WMI backend is not implemented yet.".to_string())
}

pub fn apply_manual_fans(_cpu_speed: u16, _gpu_speed: u16) -> Result<(), String> {
    Err("Windows EC/WMI backend is not implemented yet.".to_string())
}

pub fn apply_curve_fans_pct(_pct: u16) -> Result<(), String> {
    Err("Windows EC/WMI backend is not implemented yet.".to_string())
}

pub fn notify(_summary: &str, _body: &str) {}

pub fn is_autostart_enabled() -> bool {
    false
}

pub fn set_autostart(_enabled: bool) -> Result<(), String> {
    Err("Windows autostart is not implemented yet.".to_string())
}

pub struct CpuSampler;

impl CpuSampler {
    pub fn new() -> Self {
        Self
    }
}

pub fn read_telemetry(_cpu_sampler: &mut CpuSampler) -> Telemetry {
    Telemetry::default()
}
