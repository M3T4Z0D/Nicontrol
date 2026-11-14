use super::{
    pct_to_raw, raw_to_pct, save_mode, FanInfo, HardwareInfo, PowerMode, Telemetry,
};
use crate::i18n::{load_lang, strings, Lang};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PLATFORM_PROFILE_PATH: &str = "/sys/firmware/acpi/platform_profile";

const LINUWU_THERMAL_CANDIDATES: &[&str] = &[
    "/sys/devices/platform/acer-wmi/nitro_sense/thermal_profile",
    "/sys/devices/platform/acer-wmi/predator_sense/thermal_profile",
];

fn find_linuwu_thermal_path() -> Option<&'static str> {
    LINUWU_THERMAL_CANDIDATES
        .iter()
        .copied()
        .find(|p| Path::new(p).exists())
}

fn is_nvidia_gpu_suspended() -> bool {
    let Ok(entries) = fs::read_dir("/sys/bus/pci/devices") else {
        return false;
    };
    let mut found_nvidia = false;
    for entry in entries.flatten() {
        let dev = entry.path();
        let Ok(vendor) = fs::read_to_string(dev.join("vendor")) else {
            continue;
        };
        if vendor.trim() == "0x10de" {
            found_nvidia = true;
            if let Ok(status) = fs::read_to_string(dev.join("power/runtime_status")) {
                if status.trim() != "suspended" {
                    return false;
                }
            } else {
                return false;
            }
        }
    }
    found_nvidia
}

pub fn detect_hardware_info() -> HardwareInfo {
    let laptop_model = fs::read_to_string("/sys/class/dmi/id/product_name")
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Nitro Series".to_string());

    let cpu_model = fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|content| {
            content.lines().find_map(|line| {
                let (key, val) = line.split_once(':')?;
                if key.trim() == "model name" {
                    Some(val.trim().to_string())
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| "CPU".to_string());

    let gpu_model = if is_nvidia_gpu_suspended() {
        "NVIDIA dGPU (Suspended)".to_string()
    } else {
        Command::new("nvidia-smi")
            .args(["--query-gpu=name", "--format=csv,noheader"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "GPU".to_string())
    };

    HardwareInfo {
        laptop_model,
        cpu_model,
        gpu_model,
    }
}

fn write_device(path: &str, value: &str) -> Result<(), String> {
    let lang = load_lang();
    fs::write(path, format!("{value}\n")).map_err(|e| match (lang, e.kind()) {
        (Lang::PtBr, std::io::ErrorKind::PermissionDenied) => format!(
            "Sem permissão em {path}. Configure uma regra udev ou instale o pacote .deb."
        ),
        (Lang::PtBr, std::io::ErrorKind::NotFound) => format!(
            "Dispositivo {path} não encontrado. Verifique se o driver EC está carregado."
        ),
        (Lang::PtBr, _) => format!("Erro ao acessar {path}: {e}"),
        (Lang::En, std::io::ErrorKind::PermissionDenied) => format!(
            "Permission denied on {path}. Configure a udev rule or install via the .deb package."
        ),
        (Lang::En, std::io::ErrorKind::NotFound) => format!(
            "Device {path} not found. Check if your EC kernel driver is loaded."
        ),
        (Lang::En, _) => format!("Error accessing {path}: {e}"),
    })
}

pub fn apply_turbo() -> Result<(), String> {
    if Path::new("/dev/turbo").exists() {
        write_device("/dev/turbo", "1")?;
    } else if let Some(linuwu) = find_linuwu_thermal_path() {
        write_device(linuwu, "turbo")?;
    } else if Path::new(PLATFORM_PROFILE_PATH).exists() {
        write_device(PLATFORM_PROFILE_PATH, "performance")?;
    } else {
        return write_device("/dev/turbo", "1");
    }
    save_mode(PowerMode::Turbo);
    notify("Nicontrol", strings(load_lang()).notify_turbo);
    Ok(())
}

pub fn apply_auto() -> Result<(), String> {
    if Path::new("/dev/turbo").exists() {
        write_device("/dev/turbo", "0")?;
        write_device("/dev/fan_auto", "1")?;
    } else if let Some(linuwu) = find_linuwu_thermal_path() {
        write_device(linuwu, "balanced")?;
    } else if Path::new(PLATFORM_PROFILE_PATH).exists() {
        write_device(PLATFORM_PROFILE_PATH, "balanced")?;
    } else {
        return write_device("/dev/turbo", "0");
    }
    save_mode(PowerMode::Auto);
    notify("Nicontrol", strings(load_lang()).notify_auto);
    Ok(())
}

pub fn write_manual_fans_raw(cpu_speed: u16, gpu_speed: u16) -> Result<(), String> {
    let _ = write_device("/dev/turbo", "0");
    write_device("/dev/fan1", &cpu_speed.to_string())?;
    write_device("/dev/fan2", &gpu_speed.to_string())?;
    Ok(())
}

pub fn apply_manual_fans(cpu_speed: u16, gpu_speed: u16) -> Result<(), String> {
    write_manual_fans_raw(cpu_speed, gpu_speed)?;
    save_mode(PowerMode::Manual(cpu_speed, gpu_speed));
    let p1 = raw_to_pct(cpu_speed);
    let p2 = raw_to_pct(gpu_speed);
    let s = strings(load_lang());
    notify(
        "Nicontrol",
        &format!("{}: CPU {p1}% | GPU {p2}%", s.notify_manual_prefix),
    );
    Ok(())
}

pub fn apply_curve_fans_pct(pct: u16) -> Result<(), String> {
    let raw = pct_to_raw(pct);
    write_manual_fans_raw(raw, raw)
}

pub fn notify(summary: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "Nicontrol", "-t", "2200", summary, body])
        .spawn();
}

pub fn autostart_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/autostart/nicontrol.desktop"))
}

pub fn is_autostart_enabled() -> bool {
    autostart_path().map(|p| p.exists()).unwrap_or(false)
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let path = autostart_path().ok_or("HOME not set")?;
    if enabled {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "nicontrol".to_string());
        let content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Nicontrol\n\
             Comment=Open-source Fan & Power Profile Control Center\n\
             Exec={exe} --tray\n\
             Icon=nicontrol\n\
             Terminal=false\n\
             X-KDE-autostart-after=panel\n\
             StartupNotify=false\n"
        );
        fs::write(&path, content).map_err(|e| e.to_string())
    } else if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

pub struct CpuSampler {
    prev_idle: u64,
    prev_total: u64,
}

impl CpuSampler {
    pub fn new() -> Self {
        let (idle, total) = Self::read_raw().unwrap_or((0, 0));
        Self {
            prev_idle: idle,
            prev_total: total,
        }
    }

    fn read_raw() -> Option<(u64, u64)> {
        let content = fs::read_to_string("/proc/stat").ok()?;
        let line = content.lines().next()?;
        let nums: Vec<u64> = line
            .split_whitespace()
            .skip(1)
            .filter_map(|s| s.parse::<u64>().ok())
            .collect();
        if nums.len() < 4 {
            return None;
        }
        let idle = nums[3] + nums.get(4).copied().unwrap_or(0);
        let total: u64 = nums.iter().sum();
        Some((idle, total))
    }

    pub fn sample_usage(&mut self) -> Option<f32> {
        let (idle, total) = Self::read_raw()?;
        let d_idle = idle.saturating_sub(self.prev_idle);
        let d_total = total.saturating_sub(self.prev_total);
        self.prev_idle = idle;
        self.prev_total = total;
        if d_total == 0 {
            return Some(0.0);
        }
        let used = d_total.saturating_sub(d_idle) as f32;
        Some((used / d_total as f32) * 100.0)
    }
}

pub fn read_cpu_temp() -> Option<u32> {
    let entries = fs::read_dir("/sys/class/hwmon").ok()?;
    let mut fallback_acpitz: Option<u32> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(name) = fs::read_to_string(path.join("name")) {
            let drv = name.trim();
            let is_primary = matches!(drv, "coretemp" | "k10temp" | "zenpower" | "zenpower3");
            if is_primary || drv == "acpitz" {
                if let Ok(temp_raw) = fs::read_to_string(path.join("temp1_input")) {
                    if let Ok(milli_c) = temp_raw.trim().parse::<u32>() {
                        let c = milli_c / 1000;
                        if is_primary {
                            return Some(c);
                        }
                        fallback_acpitz.get_or_insert(c);
                    }
                }
            }
        }
    }
    fallback_acpitz
}

pub fn read_fans() -> Vec<FanInfo> {
    let mut fans = Vec::new();

    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();
            for idx in 1..=8 {
                let input_path = path.join(format!("fan{idx}_input"));
                if !input_path.exists() {
                    continue;
                }
                let rpm = fs::read_to_string(&input_path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok());
                let label = fs::read_to_string(path.join(format!("fan{idx}_label")))
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| match idx {
                        1 => "CPU Fan".to_string(),
                        2 => "GPU Fan".to_string(),
                        _ => format!("Fan {idx}"),
                    });
                let pct = fs::read_to_string(path.join(format!("pwm{idx}")))
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .map(|pwm| ((pwm.min(255) * 100 + 127) / 255) as u16);

                fans.push(FanInfo {
                    name: label,
                    rpm,
                    pct,
                });
            }
        }
    }

    if fans.is_empty() {
        for idx in 1..=4 {
            let dev_path = format!("/dev/fan{idx}");
            if Path::new(&dev_path).exists() {
                let name = match idx {
                    1 => "CPU Fan (fan1)".to_string(),
                    2 => "GPU Fan (fan2)".to_string(),
                    _ => format!("Fan {idx} (fan{idx})"),
                };
                fans.push(FanInfo {
                    name,
                    rpm: None,
                    pct: None,
                });
            }
        }
    }

    fans
}

pub fn read_telemetry(cpu_sampler: &mut CpuSampler) -> Telemetry {
    let fans = read_fans();
    let suspended = is_nvidia_gpu_suspended();

    let mut tel = Telemetry {
        cpu_temp_c: read_cpu_temp(),
        cpu_usage_pct: cpu_sampler.sample_usage(),
        gpu_suspended: suspended,
        fans,
        ..Default::default()
    };

    if suspended {
        tel.gpu_power_w = Some(0.0);
        return tel;
    }

    if let Ok(output) = Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,temperature.gpu,power.draw,clocks.current.graphics,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            let parts: Vec<&str> = text.trim().split(',').collect();
            if parts.len() == 6 {
                tel.gpu_usage_pct = parts[0].trim().parse::<f32>().ok();
                tel.gpu_temp_c = parts[1].trim().parse::<u32>().ok();
                tel.gpu_power_w = parts[2].trim().parse::<f32>().ok();
                tel.gpu_clock_mhz = parts[3].trim().parse::<u32>().ok();
                tel.gpu_vram_mb = parts[4].trim().parse::<u32>().ok();
                tel.gpu_vram_total_mb = parts[5].trim().parse::<u32>().ok();
            }
        }
    }

    tel
}
