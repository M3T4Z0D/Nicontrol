#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::{
    apply_auto, apply_curve_fans_pct, apply_manual_fans, apply_turbo, detect_hardware_info,
    get_battery_limit_state, get_kbd_backlight_state, is_autostart_enabled, is_gamemode_active,
    notify, read_telemetry, reapply_saved_mode, set_autostart, set_battery_limit_80,
    set_kbd_brightness, CpuSampler,
};
#[cfg(target_os = "windows")]
pub use windows::{
    apply_auto, apply_curve_fans_pct, apply_manual_fans, apply_turbo, detect_hardware_info,
    get_battery_limit_state, get_kbd_backlight_state, is_autostart_enabled, is_gamemode_active,
    notify, read_telemetry, reapply_saved_mode, set_autostart, set_battery_limit_80,
    set_kbd_brightness, CpuSampler,
};

use crate::i18n::{load_lang, strings, Lang};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct HardwareInfo {
    pub laptop_model: String,
    pub cpu_model: String,
    pub gpu_model: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PowerMode {
    Auto,
    Turbo,
    Manual(u16, u16),
    Curve,
}

impl PowerMode {
    pub fn label(&self, lang: Lang) -> String {
        let s = strings(lang);
        match self {
            PowerMode::Auto => s.mode_auto_short.to_string(),
            PowerMode::Turbo => s.mode_turbo_short.to_string(),
            PowerMode::Manual(f1, f2) => {
                let p1 = raw_to_pct(*f1);
                let p2 = raw_to_pct(*f2);
                format!("{} (CPU {p1}% / GPU {p2}%)", s.mode_manual_fmt_prefix)
            }
            PowerMode::Curve => s.mode_curve_short.to_string(),
        }
    }

    pub fn serialize(&self) -> String {
        match self {
            PowerMode::Auto => "auto".to_string(),
            PowerMode::Turbo => "turbo".to_string(),
            PowerMode::Manual(f1, f2) => format!("manual:{f1}:{f2}"),
            PowerMode::Curve => "curve".to_string(),
        }
    }

    pub fn deserialize(s: &str) -> Self {
        let s = s.trim();
        if s == "turbo" {
            PowerMode::Turbo
        } else if s == "curve" {
            PowerMode::Curve
        } else if let Some(rest) = s.strip_prefix("manual:") {
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(f1), Ok(f2)) = (parts[0].parse::<u16>(), parts[1].parse::<u16>()) {
                    return PowerMode::Manual(f1, f2);
                }
            }
            PowerMode::Auto
        } else {
            PowerMode::Auto
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FanInfo {
    pub name: String,
    pub rpm: Option<u32>,
    pub pct: Option<u16>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BatteryLimitState {
    pub supported: bool,
    pub enabled_80: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KbdBacklightState {
    pub supported: bool,
    pub brightness: u32,
    pub max_brightness: u32,
}

#[derive(Clone, Default, Debug)]
pub struct Telemetry {
    pub cpu_temp_c: Option<u32>,
    pub cpu_usage_pct: Option<f32>,
    pub cpu_freq_mhz: Option<u32>,
    pub ram_used_mb: Option<u32>,
    pub ram_total_mb: Option<u32>,
    pub gpu_temp_c: Option<u32>,
    pub gpu_usage_pct: Option<f32>,
    pub gpu_power_w: Option<f32>,
    pub gpu_clock_mhz: Option<u32>,
    pub gpu_vram_mb: Option<u32>,
    pub gpu_vram_total_mb: Option<u32>,
    pub gpu_suspended: bool,
    pub fans: Vec<FanInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FanCurvePoint {
    pub temp_c: u32,
    pub fan_pct: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FanCurve {
    pub points: Vec<FanCurvePoint>,
}

impl Default for FanCurve {
    fn default() -> Self {
        Self {
            points: vec![
                FanCurvePoint { temp_c: 45, fan_pct: 30 },
                FanCurvePoint { temp_c: 60, fan_pct: 50 },
                FanCurvePoint { temp_c: 75, fan_pct: 75 },
                FanCurvePoint { temp_c: 85, fan_pct: 100 },
            ],
        }
    }
}

impl FanCurve {
    pub fn evaluate(&self, temp_c: u32) -> u16 {
        if self.points.is_empty() {
            return 50;
        }
        let mut sorted = self.points.clone();
        sorted.sort_by_key(|p| p.temp_c);

        if temp_c <= sorted[0].temp_c {
            return sorted[0].fan_pct.min(100);
        }
        let last = sorted[sorted.len() - 1];
        if temp_c >= last.temp_c {
            return last.fan_pct.min(100);
        }

        for pair in sorted.windows(2) {
            let a = pair[0];
            let b = pair[1];
            if temp_c >= a.temp_c && temp_c <= b.temp_c {
                if b.temp_c == a.temp_c {
                    return b.fan_pct.min(100);
                }
                let ratio = (temp_c - a.temp_c) as f32 / (b.temp_c - a.temp_c) as f32;
                let pct = a.fan_pct as f32 + ratio * (b.fan_pct as f32 - a.fan_pct as f32);
                return pct.round().clamp(0.0, 100.0) as u16;
            }
        }
        50
    }

    pub fn serialize(&self) -> String {
        self.points
            .iter()
            .map(|p| format!("{}:{}", p.temp_c, p.fan_pct.min(100)))
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn deserialize(s: &str) -> Option<Self> {
        let pts: Vec<FanCurvePoint> = s
            .trim()
            .split(',')
            .filter_map(|item| {
                let (t, f) = item.split_once(':')?;
                Some(FanCurvePoint {
                    temp_c: t.trim().parse::<u32>().ok()?,
                    fan_pct: f.trim().parse::<u16>().ok()?.min(100),
                })
            })
            .collect();
        if pts.len() >= 2 {
            Some(Self { points: pts })
        } else {
            None
        }
    }
}

pub fn curve_config_path() -> Option<PathBuf> {
    if let Some(cfg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(cfg).join("nicontrol").join("curve.conf"));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/nicontrol/curve.conf"))
}

pub fn load_curve() -> FanCurve {
    curve_config_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| FanCurve::deserialize(&s))
        .unwrap_or_default()
}

pub fn save_curve(curve: &FanCurve) {
    if let Some(path) = curve_config_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, curve.serialize());
    }
}

pub fn apply_curve_mode() -> Result<(), String> {
    save_mode(PowerMode::Curve);
    notify("Nicontrol", strings(load_lang()).notify_curve);
    Ok(())
}

pub fn auto_game_config_path() -> Option<PathBuf> {
    if let Some(cfg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(cfg).join("nicontrol").join("auto_game"));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/nicontrol/auto_game"))
}

pub fn is_auto_game_enabled() -> bool {
    auto_game_config_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
}

pub fn set_auto_game_enabled(enabled: bool) {
    if let Some(path) = auto_game_config_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, if enabled { "1" } else { "0" });
    }
}

pub fn state_file_path() -> PathBuf {
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir).join("nicontrol").join("mode");
    }
    if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(state_home).join("nicontrol").join("mode");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/state/nicontrol/mode");
    }
    PathBuf::from("/tmp/nicontrol-mode")
}

pub fn load_saved_mode() -> PowerMode {
    fs::read_to_string(state_file_path())
        .map(|s| PowerMode::deserialize(&s))
        .unwrap_or(PowerMode::Auto)
}

pub fn save_mode(mode: PowerMode) {
    let path = state_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, mode.serialize());
}

pub fn raw_to_pct(raw: u16) -> u16 {
    (((raw as u32 * 100) + 1280) / 2560).min(100) as u16
}

pub fn pct_to_raw(pct: u16) -> u16 {
    ((pct.min(100) as u32 * 2560) / 100) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_mode_roundtrip() {
        assert_eq!(PowerMode::deserialize(&PowerMode::Auto.serialize()), PowerMode::Auto);
        assert_eq!(PowerMode::deserialize(&PowerMode::Turbo.serialize()), PowerMode::Turbo);
        assert_eq!(PowerMode::deserialize(&PowerMode::Curve.serialize()), PowerMode::Curve);
        assert_eq!(
            PowerMode::deserialize(&PowerMode::Manual(1280, 2048).serialize()),
            PowerMode::Manual(1280, 2048)
        );
        assert_eq!(PowerMode::deserialize("invalid"), PowerMode::Auto);
    }

    #[test]
    fn fan_pct_raw_rounding() {
        for pct in (0..=100).step_by(5) {
            let raw = pct_to_raw(pct);
            assert_eq!(raw_to_pct(raw), pct);
        }
        let raw_33 = pct_to_raw(33);
        assert_eq!(raw_to_pct(raw_33), 33);
    }

    #[test]
    fn fan_curve_interpolation_and_serialization() {
        let curve = FanCurve::default();
        assert_eq!(curve.evaluate(30), 30);
        assert_eq!(curve.evaluate(45), 30);
        assert_eq!(curve.evaluate(60), 50);
        assert_eq!(curve.evaluate(75), 75);
        assert_eq!(curve.evaluate(85), 100);
        assert_eq!(curve.evaluate(95), 100);
        // Midpoint between (60, 50) and (75, 75) -> ~67.5 -> 63
        let mid = curve.evaluate(67);
        assert!((60..=64).contains(&mid));

        let serialized = curve.serialize();
        let parsed = FanCurve::deserialize(&serialized).unwrap();
        assert_eq!(parsed, curve);
    }

    #[test]
    fn state_path_is_non_empty() {
        let p = state_file_path();
        assert!(p.ends_with("mode") || p.ends_with("nicontrol-mode"));
    }
}
