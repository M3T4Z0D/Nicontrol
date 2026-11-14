use crate::hardware::{
    apply_auto, apply_curve_fans_pct, apply_curve_mode, apply_manual_fans, apply_turbo,
    is_auto_game_enabled, is_gamemode_active, load_curve, load_saved_mode, notify, pct_to_raw,
    read_telemetry, reapply_saved_mode, CpuSampler, PowerMode, Telemetry,
};
use crate::i18n::{load_lang, strings};
use ksni::blocking::TrayMethods;
use ksni::{Icon, MenuItem, Tray};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

fn tray_pid_path() -> PathBuf {
    crate::hardware::state_file_path()
        .parent()
        .map(|p| p.join("tray.pid"))
        .unwrap_or_else(|| PathBuf::from("/tmp/nicontrol-tray.pid"))
}

pub fn is_tray_running() -> bool {
    let Ok(pid_str) = fs::read_to_string(tray_pid_path()) else {
        return false;
    };
    let Ok(pid) = pid_str.trim().parse::<u32>() else {
        return false;
    };
    if pid == std::process::id() {
        return false;
    }
    let cmdline_path = format!("/proc/{pid}/cmdline");
    let Ok(cmdline) = fs::read(&cmdline_path) else {
        return false;
    };
    let text = String::from_utf8_lossy(&cmdline);
    text.contains("nicontrol") && text.contains("--tray")
}

pub fn ensure_tray_running() {
    if is_tray_running() {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg("--tray").spawn();
    }
}

fn write_tray_pid() {
    let path = tray_pid_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, std::process::id().to_string());
}

fn remove_tray_pid() {
    let _ = fs::remove_file(tray_pid_path());
}

fn close_gui_if_running() {
    let gui_pid = crate::hardware::state_file_path()
        .parent()
        .map(|p| p.join("gui.pid"))
        .unwrap_or_else(|| PathBuf::from("/tmp/nicontrol-gui.pid"));

    let Ok(pid_str) = fs::read_to_string(&gui_pid) else {
        return;
    };
    let Ok(pid) = pid_str.trim().parse::<u32>() else {
        return;
    };
    if let Ok(cmdline) = fs::read(format!("/proc/{pid}/cmdline")) {
        let text = String::from_utf8_lossy(&cmdline);
        if text.contains("nicontrol") && !text.contains("--tray") {
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status();
        }
    }
    let _ = fs::remove_file(gui_pid);
}

struct TrayState {
    mode: PowerMode,
    telemetry: Telemetry,
}

struct NitroTray {
    state: Arc<Mutex<TrayState>>,
}

fn open_gui_window() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg("--gui").spawn();
    }
}

/// Generates a 22x22 ARGB32 "N" crystal badge icon following the Dark Crystal Blue theme.
fn build_crystal_icon(mode: PowerMode) -> Icon {
    const SIZE: i32 = 22;
    let (bg_r, bg_g, bg_b, ring_r, ring_g, ring_b) = match mode {
        // Dark Crystal Navy (Subtle / Auto)
        PowerMode::Auto => (0x16, 0x2a, 0x45, 0x3b, 0x82, 0xf6),
        // Vibrant Orange-Red (Turbo Active)
        PowerMode::Turbo => (0xea, 0x43, 0x0c, 0xfb, 0x92, 0x3c),
        // Deep Indigo Crystal (Manual)
        PowerMode::Manual(_, _) => (0x31, 0x2e, 0x81, 0x81, 0x8c, 0xf8),
        // Teal Crystal (Fan Curve)
        PowerMode::Curve => (0x0f, 0x4c, 0x5c, 0x2d, 0xd4, 0xbf),
    };

    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let center = (SIZE as f32 - 1.0) / 2.0;
    let radius = 9.5_f32;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();

            let in_left = (6..=8).contains(&x) && (5..=16).contains(&y);
            let in_right = (13..=15).contains(&x) && (5..=16).contains(&y);
            let in_diag = (8..=13).contains(&x) && (5..=16).contains(&y) && {
                let expected_y = 5.0 + ((x - 8) as f32) * (11.0 / 5.0);
                (y as f32 - expected_y).abs() <= 1.25
            };
            let is_letter = in_left || in_right || in_diag;

            if dist <= radius {
                let alpha = if dist > radius - 1.0 {
                    ((radius - dist).clamp(0.0, 1.0) * 255.0) as u8
                } else {
                    255
                };
                if is_letter {
                    data.extend_from_slice(&[alpha, 240, 248, 255]);
                } else if dist >= radius - 2.0 {
                    data.extend_from_slice(&[alpha, ring_r, ring_g, ring_b]);
                } else {
                    data.extend_from_slice(&[alpha, bg_r, bg_g, bg_b]);
                }
            } else {
                data.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    Icon {
        width: SIZE,
        height: SIZE,
        data,
    }
}

impl Tray for NitroTray {
    fn id(&self) -> String {
        "nicontrol".to_string()
    }

    fn title(&self) -> String {
        let st = self.state.lock().unwrap();
        let lang = load_lang();
        format!("Nicontrol — {}", st.mode.label(lang))
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        let st = self.state.lock().unwrap();
        vec![build_crystal_icon(st.mode)]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let st = self.state.lock().unwrap();
        let lang = load_lang();
        let s = strings(lang);
        let cpu = st
            .telemetry
            .cpu_temp_c
            .map(|t| format!("{t}°C"))
            .unwrap_or_else(|| "--".to_string());
        let gpu = st
            .telemetry
            .gpu_temp_c
            .map(|t| format!("{t}°C"))
            .unwrap_or_else(|| "--".to_string());
        let watts = st
            .telemetry
            .gpu_power_w
            .map(|w| format!("{w:.0}W"))
            .unwrap_or_else(|| "--".to_string());

        ksni::ToolTip {
            title: format!("Nicontrol — {}", st.mode.label(lang)),
            description: format!("CPU: {cpu}  |  GPU: {gpu} ({watts})\n{}", s.tray_tooltip_hint),
            icon_name: String::new(),
            icon_pixmap: vec![],
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let current = self.state.lock().unwrap().mode;
        match current {
            PowerMode::Turbo => apply_and_update(&self.state, apply_auto(), PowerMode::Auto),
            _ => apply_and_update(&self.state, apply_turbo(), PowerMode::Turbo),
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        use ksni::menu::*;

        let lang = load_lang();
        let s = strings(lang);
        let st = self.state.lock().unwrap();
        let cpu_str = st
            .telemetry
            .cpu_temp_c
            .map(|t| format!("{t}°C"))
            .unwrap_or_else(|| "--".to_string());
        let gpu_str = st
            .telemetry
            .gpu_temp_c
            .map(|t| format!("{t}°C"))
            .unwrap_or_else(|| "--".to_string());
        let power_str = st
            .telemetry
            .gpu_power_w
            .map(|w| format!("{w:.1}W"))
            .unwrap_or_else(|| "--".to_string());

        let is_turbo = st.mode == PowerMode::Turbo;
        let is_auto = st.mode == PowerMode::Auto;
        let is_curve = st.mode == PowerMode::Curve;
        let current_label = format!("{}: {}", s.tray_status_prefix, st.mode.label(lang));
        let gpu_info = if st.telemetry.gpu_suspended {
            s.gpu_suspended.to_string()
        } else {
            format!("{gpu_str} ({power_str})")
        };
        let sensors_label = format!("CPU: {cpu_str}  |  GPU: {gpu_info}");
        drop(st);

        vec![
            StandardItem {
                label: s.tray_open_gui.to_string(),
                icon_name: "utilities-system-monitor".to_string(),
                activate: Box::new(|_| open_gui_window()),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: current_label,
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: sensors_label,
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: s.tray_turbo_item.to_string(),
                checked: is_turbo,
                activate: Box::new(|this: &mut Self| {
                    apply_and_update(&this.state, apply_turbo(), PowerMode::Turbo);
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: s.tray_auto_item.to_string(),
                checked: is_auto,
                activate: Box::new(|this: &mut Self| {
                    apply_and_update(&this.state, apply_auto(), PowerMode::Auto);
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: s.tray_curve_item.to_string(),
                checked: is_curve,
                activate: Box::new(|this: &mut Self| {
                    apply_and_update(&this.state, apply_curve_mode(), PowerMode::Curve);
                }),
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: s.tray_manual_menu.to_string(),
                submenu: vec![
                    StandardItem {
                        label: s.tray_50.to_string(),
                        activate: Box::new(|this: &mut Self| {
                            let raw = pct_to_raw(50);
                            apply_and_update(
                                &this.state,
                                apply_manual_fans(raw, raw),
                                PowerMode::Manual(raw, raw),
                            );
                        }),
                        ..Default::default()
                    }
                    .into(),
                    StandardItem {
                        label: s.tray_75.to_string(),
                        activate: Box::new(|this: &mut Self| {
                            let raw = pct_to_raw(75);
                            apply_and_update(
                                &this.state,
                                apply_manual_fans(raw, raw),
                                PowerMode::Manual(raw, raw),
                            );
                        }),
                        ..Default::default()
                    }
                    .into(),
                    StandardItem {
                        label: s.tray_100.to_string(),
                        activate: Box::new(|this: &mut Self| {
                            let raw = pct_to_raw(100);
                            apply_and_update(
                                &this.state,
                                apply_manual_fans(raw, raw),
                                PowerMode::Manual(raw, raw),
                            );
                        }),
                        ..Default::default()
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: s.tray_quit.to_string(),
                icon_name: "application-exit".to_string(),
                activate: Box::new(|_| {
                    close_gui_if_running();
                    remove_tray_pid();
                    std::process::exit(0);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn apply_and_update(state: &Arc<Mutex<TrayState>>, res: Result<(), String>, new_mode: PowerMode) {
    match res {
        Ok(()) => state.lock().unwrap().mode = new_mode,
        Err(e) => {
            eprintln!("Nicontrol erro: {e}");
            notify(strings(load_lang()).notify_error_title, &e);
        }
    }
}

pub fn run_tray() {
    if is_tray_running() {
        return;
    }
    write_tray_pid();

    let mut sampler = CpuSampler::new();
    let initial_tel = read_telemetry(&mut sampler);

    let state = Arc::new(Mutex::new(TrayState {
        mode: load_saved_mode(),
        telemetry: initial_tel,
    }));

    let tray = NitroTray {
        state: Arc::clone(&state),
    };

    let handle = match tray.spawn() {
        Ok(h) => h,
        Err(e) => {
            eprintln!(
                "Falha ao registrar ícone na bandeja (StatusNotifierItem): {e}\n\
                 Dica: no GNOME, instale a extensão 'AppIndicator and KStatusNotifierItem Support'."
            );
            std::process::exit(1);
        }
    };

    let state_bg = Arc::clone(&state);
    thread::spawn(move || {
        let mut last_curve_pct: Option<u16> = None;
        let mut last_curve_temp: Option<u32> = None;
        let mut last_wall = SystemTime::now();
        let mut last_mono = Instant::now();
        let mut auto_game_active = false;
        let mut pre_game_mode = PowerMode::Auto;

        loop {
            // Detect resume from suspend (wall clock jumped significantly ahead of monotonic)
            let now_wall = SystemTime::now();
            let now_mono = Instant::now();
            if let Ok(wall_elapsed) = now_wall.duration_since(last_wall) {
                let mono_elapsed = now_mono.duration_since(last_mono);
                if wall_elapsed > mono_elapsed + Duration::from_secs(8) {
                    reapply_saved_mode();
                }
            }
            last_wall = now_wall;
            last_mono = now_mono;

            let tel = read_telemetry(&mut sampler);
            let mut synced_mode = load_saved_mode();

            // Auto-Turbo on GameMode
            if is_auto_game_enabled() {
                let gm_on = is_gamemode_active();
                if gm_on && !auto_game_active {
                    if synced_mode == PowerMode::Auto {
                        pre_game_mode = synced_mode;
                        if apply_turbo().is_ok() {
                            synced_mode = PowerMode::Turbo;
                            auto_game_active = true;
                        }
                    }
                } else if !gm_on && auto_game_active {
                    if pre_game_mode == PowerMode::Auto && apply_auto().is_ok() {
                        synced_mode = PowerMode::Auto;
                    }
                    auto_game_active = false;
                }
            } else {
                auto_game_active = false;
            }

            if synced_mode == PowerMode::Curve {
                let max_temp = tel
                    .cpu_temp_c
                    .unwrap_or(0)
                    .max(tel.gpu_temp_c.unwrap_or(0));
                if max_temp > 0 {
                    let temp_changed_enough = last_curve_temp
                        .map(|prev| max_temp.abs_diff(prev) >= 3)
                        .unwrap_or(true);
                    if temp_changed_enough {
                        let curve = load_curve();
                        let target_pct = curve.evaluate(max_temp);
                        if last_curve_pct != Some(target_pct) {
                            let _ = apply_curve_fans_pct(target_pct);
                            last_curve_pct = Some(target_pct);
                        }
                        last_curve_temp = Some(max_temp);
                    }
                }
            } else {
                last_curve_pct = None;
                last_curve_temp = None;
            }

            {
                let mut st = state_bg.lock().unwrap();
                st.telemetry = tel;
                st.mode = synced_mode;
            }
            handle.update(|_| {});
            thread::sleep(Duration::from_secs(2));
        }
    });

    loop {
        thread::park();
    }
}
