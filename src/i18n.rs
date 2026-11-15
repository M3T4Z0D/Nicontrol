use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    PtBr,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::PtBr => "pt-BR",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        let lower = s.trim().to_ascii_lowercase();
        if lower.starts_with("pt") {
            Lang::PtBr
        } else {
            Lang::En
        }
    }
}

pub fn detect_system_lang() -> Lang {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            if !val.is_empty() {
                return Lang::from_str_lossy(&val);
            }
        }
    }
    Lang::En
}

fn lang_config_path() -> Option<PathBuf> {
    if let Some(cfg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(cfg).join("nicontrol").join("lang"));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/nicontrol/lang"))
}

pub fn load_lang() -> Lang {
    if let Some(path) = lang_config_path() {
        if let Ok(content) = fs::read_to_string(path) {
            return Lang::from_str_lossy(&content);
        }
    }
    detect_system_lang()
}

pub fn save_lang(lang: Lang) {
    if let Some(path) = lang_config_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, lang.code());
    }
}

pub struct Strings {
    pub active_profile: &'static str,
    pub start_in_tray: &'static str,

    pub mode_auto_short: &'static str,
    pub mode_turbo_short: &'static str,
    pub mode_manual_fmt_prefix: &'static str,
    pub mode_curve_short: &'static str,

    pub card_auto_title: &'static str,
    pub card_auto_desc: &'static str,
    pub btn_auto_active: &'static str,
    pub btn_auto_activate: &'static str,

    pub card_turbo_title: &'static str,
    pub card_turbo_desc: &'static str,
    pub btn_turbo_active: &'static str,
    pub btn_turbo_activate: &'static str,

    pub card_manual_title: &'static str,
    pub card_manual_desc: &'static str,
    pub btn_manual_active: &'static str,
    pub btn_manual_apply: &'static str,

    pub card_curve_title: &'static str,
    pub card_curve_desc: &'static str,
    pub btn_curve_active: &'static str,
    pub btn_curve_activate: &'static str,
    pub curve_editor_title: &'static str,
    pub curve_point_prefix: &'static str,
    pub curve_save_apply: &'static str,

    pub cpu_usage: &'static str,
    pub ram_label: &'static str,
    pub gpu_suspended: &'static str,
    pub cpu_chart_title: &'static str,
    pub gpu_chart_title: &'static str,

    pub auto_game_mode: &'static str,
    pub battery_limit_80: &'static str,
    pub kbd_backlight: &'static str,

    pub fine_fan_title: &'static str,
    pub sync_fans: &'static str,
    pub cpu_fan_label: &'static str,
    pub gpu_fan_label: &'static str,
    pub low_fan_warning: &'static str,
    pub apply_speed: &'static str,
    pub restore_auto: &'static str,

    pub msg_autostart_on: &'static str,
    pub msg_autostart_off: &'static str,
    pub msg_auto_on: &'static str,
    pub msg_turbo_on: &'static str,
    pub msg_manual_applied: &'static str,
    pub msg_curve_on: &'static str,
    pub msg_fans_adjusted: &'static str,
    pub msg_fans_auto: &'static str,

    pub notify_turbo: &'static str,
    pub notify_auto: &'static str,
    pub notify_curve: &'static str,
    pub notify_manual_prefix: &'static str,
    pub notify_error_title: &'static str,

    pub tray_tooltip_hint: &'static str,
    pub tray_open_gui: &'static str,
    pub tray_status_prefix: &'static str,
    pub tray_turbo_item: &'static str,
    pub tray_auto_item: &'static str,
    pub tray_curve_item: &'static str,
    pub tray_manual_menu: &'static str,
    pub tray_50: &'static str,
    pub tray_75: &'static str,
    pub tray_100: &'static str,
    pub tray_quit: &'static str,

    pub update_available: &'static str,
    pub update_download: &'static str,

    pub footer_disclaimer: &'static str,
}

const EN_STRINGS: Strings = Strings {
    active_profile: "Active profile",
    start_in_tray: "Start tray widget on login",

    mode_auto_short: "Auto (Silent)",
    mode_turbo_short: "TURBO (Max)",
    mode_manual_fmt_prefix: "Manual",
    mode_curve_short: "Fan Curve",

    card_auto_title: "AUTO",
    card_auto_desc: "Quiet profile for coding, study, and daily work.",
    btn_auto_active: "AUTO ACTIVE",
    btn_auto_activate: "ENABLE AUTO",

    card_turbo_title: "TURBO MODE",
    card_turbo_desc: "Unlocks maximum GPU TGP and full fan speed.",
    btn_turbo_active: "TURBO ACTIVE",
    btn_turbo_activate: "ENABLE TURBO",

    card_manual_title: "MANUAL CONTROL",
    card_manual_desc: "Independent fine-grained CPU & GPU fan speeds.",
    btn_manual_active: "MANUAL ACTIVE",
    btn_manual_apply: "APPLY SLIDERS",

    card_curve_title: "FAN CURVE",
    card_curve_desc: "Automatic temperature-based fan speed curve.",
    btn_curve_active: "CURVE ACTIVE",
    btn_curve_activate: "ENABLE CURVE",
    curve_editor_title: "Custom Temperature Fan Curve",
    curve_point_prefix: "Point",
    curve_save_apply: "Save & Activate Curve",

    cpu_usage: "Usage",
    ram_label: "RAM",
    gpu_suspended: "Suspended (RTD3)",
    cpu_chart_title: "CPU — Usage & Temperature (60s)",
    gpu_chart_title: "GPU — Usage, Power & Temp (60s)",

    auto_game_mode: "Auto-Turbo on GameMode",
    battery_limit_80: "Limit Battery Charge to 80%",
    kbd_backlight: "Keyboard Backlight:",

    fine_fan_title: "Fine-Grained Fan Control",
    sync_fans: "Sync CPU + GPU",
    cpu_fan_label: "CPU Fan (fan1):",
    gpu_fan_label: "GPU Fan (fan2):",
    low_fan_warning: "[!] Warning: fan speeds below 20% may cause overheating under load.",
    apply_speed: "Apply Speed",
    restore_auto: "Restore Auto",

    msg_autostart_on: "Autostart enabled.",
    msg_autostart_off: "Autostart disabled.",
    msg_auto_on: "Auto mode enabled.",
    msg_turbo_on: "Turbo mode enabled!",
    msg_manual_applied: "Manual fan speed applied.",
    msg_curve_on: "Custom Fan Curve enabled!",
    msg_fans_adjusted: "Fan speeds updated.",
    msg_fans_auto: "Fans restored to Auto.",

    notify_turbo: "TURBO mode enabled! Maximum power and fan speed.",
    notify_auto: "Auto mode restored.",
    notify_curve: "Custom Fan Curve mode active.",
    notify_manual_prefix: "Manual fans",
    notify_error_title: "Nicontrol — Error",

    tray_tooltip_hint: "Left-click: toggle Turbo/Auto",
    tray_open_gui: "Open Nicontrol",
    tray_status_prefix: "Status",
    tray_turbo_item: "Turbo Mode (Max)",
    tray_auto_item: "Auto Mode (Silent)",
    tray_curve_item: "Custom Fan Curve",
    tray_manual_menu: "Manual Fans",
    tray_50: "50% (Balanced)",
    tray_75: "75% (High)",
    tray_100: "100% (Max)",
    tray_quit: "Quit",

    update_available: "New update available",
    update_download: "Download Release",

    footer_disclaimer: "Unofficial community open-source tool (not affiliated with Acer Inc.). Use at your own risk.",
};

const PT_BR_STRINGS: Strings = Strings {
    active_profile: "Perfil ativo",
    start_in_tray: "Iniciar widget na bandeja",

    mode_auto_short: "Automático (Silencioso)",
    mode_turbo_short: "TURBO (Máximo)",
    mode_manual_fmt_prefix: "Manual",
    mode_curve_short: "Curva de Fan",

    card_auto_title: "AUTOMÁTICO",
    card_auto_desc: "Silencioso para trabalho, código e estudo.",
    btn_auto_active: "MODO NORMAL ATIVO",
    btn_auto_activate: "ATIVAR AUTOMÁTICO",

    card_turbo_title: "MODO TURBO",
    card_turbo_desc: "Libera TGP máximo e coolers no máximo.",
    btn_turbo_active: "TURBO ATIVO",
    btn_turbo_activate: "ATIVAR TURBO",

    card_manual_title: "CONTROLE MANUAL",
    card_manual_desc: "Ajuste fino independente de CPU e GPU Fan.",
    btn_manual_active: "MANUAL ATIVO",
    btn_manual_apply: "APLICAR SLIDERS",

    card_curve_title: "CURVA DE FAN",
    card_curve_desc: "Curva automática por temperatura.",
    btn_curve_active: "CURVA ATIVA",
    btn_curve_activate: "ATIVAR CURVA",
    curve_editor_title: "Curva de Ventoinha por Temperatura",
    curve_point_prefix: "Ponto",
    curve_save_apply: "Salvar e Ativar Curva",

    cpu_usage: "Uso",
    ram_label: "RAM",
    gpu_suspended: "Suspensa (RTD3)",
    cpu_chart_title: "CPU — Uso & Temperatura (60s)",
    gpu_chart_title: "GPU — Uso, Potência & Temp (60s)",

    auto_game_mode: "Auto-Turbo no GameMode",
    battery_limit_80: "Limitar Bateria em 80%",
    kbd_backlight: "Brilho do Teclado:",

    fine_fan_title: "Controle Fino das Ventoinhas",
    sync_fans: "Sincronizar CPU + GPU",
    cpu_fan_label: "CPU Fan (fan1):",
    gpu_fan_label: "GPU Fan (fan2):",
    low_fan_warning: "[!] Atenção: ventoinhas abaixo de 20% podem causar superaquecimento sob carga.",
    apply_speed: "Aplicar Velocidade",
    restore_auto: "Restaurar Auto",

    msg_autostart_on: "Inicialização automática ativada.",
    msg_autostart_off: "Inicialização automática desativada.",
    msg_auto_on: "Modo Automático ativado.",
    msg_turbo_on: "Modo Turbo ativado!",
    msg_manual_applied: "Velocidade manual aplicada.",
    msg_curve_on: "Curva de Fan ativada!",
    msg_fans_adjusted: "Ventoinhas ajustadas.",
    msg_fans_auto: "Ventoinhas no automático.",

    notify_turbo: "Modo TURBO ativado! Potência e coolers no máximo.",
    notify_auto: "Modo Automático restaurado.",
    notify_curve: "Modo Curva de Fan ativado.",
    notify_manual_prefix: "Coolers manuais",
    notify_error_title: "Nicontrol — Erro",

    tray_tooltip_hint: "Clique esquerdo: alternar Turbo/Auto",
    tray_open_gui: "Abrir Painel Nicontrol",
    tray_status_prefix: "Status",
    tray_turbo_item: "Modo Turbo (Máximo)",
    tray_auto_item: "Modo Automático (Silencioso)",
    tray_curve_item: "Curva de Fan Customizada",
    tray_manual_menu: "Ventoinhas Manuais",
    tray_50: "50% (Moderado)",
    tray_75: "75% (Forte)",
    tray_100: "100% (Turbina Máxima)",
    tray_quit: "Sair",

    update_available: "Nova atualização disponível",
    update_download: "Baixar Release",

    footer_disclaimer: "Projeto open-source comunitário e não oficial (sem afiliação com a Acer Inc.). Use por sua conta e risco.",
};

pub fn strings(lang: Lang) -> &'static Strings {
    match lang {
        Lang::En => &EN_STRINGS,
        Lang::PtBr => &PT_BR_STRINGS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_parsing() {
        assert_eq!(Lang::from_str_lossy("pt_BR.UTF-8"), Lang::PtBr);
        assert_eq!(Lang::from_str_lossy("pt-BR"), Lang::PtBr);
        assert_eq!(Lang::from_str_lossy("en_US.UTF-8"), Lang::En);
        assert_eq!(Lang::from_str_lossy("C"), Lang::En);
    }
}
