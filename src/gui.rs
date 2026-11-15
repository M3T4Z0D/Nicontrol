use crate::hardware::{
    apply_auto, apply_curve_mode, apply_manual_fans, apply_turbo, detect_hardware_info,
    get_battery_limit_state, get_kbd_backlight_state, is_auto_game_enabled, is_autostart_enabled,
    load_curve, load_saved_mode, pct_to_raw, raw_to_pct, read_telemetry, save_curve,
    set_auto_game_enabled, set_autostart, set_battery_limit_80, set_kbd_brightness,
    state_file_path, BatteryLimitState, CpuSampler, FanCurve, HardwareInfo, KbdBacklightState,
    PowerMode, Telemetry,
};
use crate::i18n::{load_lang, save_lang, strings, Lang};
use crate::updater::{check_for_update, open_url, UpdateInfo, CURRENT_VERSION};
use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, RichText, Stroke, Vec2};
use std::collections::VecDeque;
use std::f32::consts::TAU;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// Dark Crystal Blue Palette (Acer Nitro V15 lid inspired)
const BG_OBSIDIAN: Color32 = Color32::from_rgb(7, 11, 19);
const CARD_CRYSTAL: Color32 = Color32::from_rgba_premultiplied(13, 22, 36, 224);
const CARD_CRYSTAL_ACTIVE: Color32 = Color32::from_rgba_premultiplied(14, 42, 74, 236);
const BORDER_CRYSTAL: Color32 = Color32::from_rgb(30, 58, 95);
const ACCENT_ICE: Color32 = Color32::from_rgb(56, 189, 248);
const ACCENT_COBALT: Color32 = Color32::from_rgb(2, 132, 199);
const ACCENT_CYAN_SOFT: Color32 = Color32::from_rgb(125, 211, 252);
const ACCENT_INDIGO: Color32 = Color32::from_rgb(129, 140, 248);
const ACCENT_TURBO_GLOW: Color32 = Color32::from_rgb(251, 146, 60);
const TEXT_ICE: Color32 = Color32::from_rgb(240, 249, 255);
const TEXT_MUTED: Color32 = Color32::from_rgb(148, 163, 184);

const HISTORY_LEN: usize = 60;

struct SharedGuiData {
    synced_mode: PowerMode,
    telemetry: Telemetry,
    update_info: Option<UpdateInfo>,
    cpu_usage_hist: VecDeque<f64>,
    gpu_usage_hist: VecDeque<f64>,
    gpu_power_hist: VecDeque<f64>,
    cpu_temp_hist: VecDeque<f64>,
    gpu_temp_hist: VecDeque<f64>,
}

struct NitroApp {
    mode: PowerMode,
    lang: Lang,
    hw_info: HardwareInfo,
    autostart: bool,
    auto_game: bool,
    battery_limit: BatteryLimitState,
    kbd_backlight: KbdBacklightState,
    cpu_fan_pct: u16,
    gpu_fan_pct: u16,
    link_fans: bool,
    fan_curve: FanCurve,
    status_msg: Option<String>,
    shared: Arc<Mutex<SharedGuiData>>,
    visible_flag: Arc<AtomicBool>,
}

impl NitroApp {
    fn new(cc: &eframe::CreationContext<'_>, visible_flag: Arc<AtomicBool>) -> Self {
        apply_crystal_theme(&cc.egui_ctx);

        let hw_info = detect_hardware_info();
        let initial_mode = load_saved_mode();
        let fan_curve = load_curve();
        let (cpu_pct, gpu_pct) = match initial_mode {
            PowerMode::Manual(f1, f2) => (raw_to_pct(f1), raw_to_pct(f2)),
            PowerMode::Turbo => (100, 100),
            PowerMode::Auto | PowerMode::Curve => (50, 50),
        };

        let mut sampler = CpuSampler::new();
        let initial_tel = read_telemetry(&mut sampler);

        let init_cpu_u = initial_tel.cpu_usage_pct.unwrap_or(0.0) as f64;
        let init_gpu_u = initial_tel.gpu_usage_pct.unwrap_or(0.0) as f64;
        let init_gpu_p = initial_tel.gpu_power_w.unwrap_or(0.0) as f64;
        let init_cpu_t = initial_tel.cpu_temp_c.unwrap_or(0) as f64;
        let init_gpu_t = initial_tel.gpu_temp_c.unwrap_or(0) as f64;

        let shared = Arc::new(Mutex::new(SharedGuiData {
            synced_mode: initial_mode,
            telemetry: initial_tel,
            update_info: None,
            cpu_usage_hist: VecDeque::from(vec![init_cpu_u; HISTORY_LEN]),
            gpu_usage_hist: VecDeque::from(vec![init_gpu_u; HISTORY_LEN]),
            gpu_power_hist: VecDeque::from(vec![init_gpu_p; HISTORY_LEN]),
            cpu_temp_hist: VecDeque::from(vec![init_cpu_t; HISTORY_LEN]),
            gpu_temp_hist: VecDeque::from(vec![init_gpu_t; HISTORY_LEN]),
        }));

        let shared_upd = Arc::clone(&shared);
        let ctx_upd = cc.egui_ctx.clone();
        thread::spawn(move || {
            if let Some(info) = check_for_update() {
                shared_upd.lock().unwrap().update_info = Some(info);
                ctx_upd.request_repaint();
            }
        });

        let shared_bg = Arc::clone(&shared);
        let ctx_bg = cc.egui_ctx.clone();
        let vis_bg = Arc::clone(&visible_flag);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(1));
            if !vis_bg.load(Ordering::Relaxed) {
                continue;
            }
            let tel = read_telemetry(&mut sampler);
            let mode_from_disk = load_saved_mode();
            {
                let mut data = shared_bg.lock().unwrap();
                push_sample(&mut data.cpu_usage_hist, tel.cpu_usage_pct.unwrap_or(0.0) as f64);
                push_sample(&mut data.gpu_usage_hist, tel.gpu_usage_pct.unwrap_or(0.0) as f64);
                push_sample(&mut data.gpu_power_hist, tel.gpu_power_w.unwrap_or(0.0) as f64);
                push_sample(&mut data.cpu_temp_hist, tel.cpu_temp_c.unwrap_or(0) as f64);
                push_sample(&mut data.gpu_temp_hist, tel.gpu_temp_c.unwrap_or(0) as f64);
                data.telemetry = tel;
                data.synced_mode = mode_from_disk;
            }
            ctx_bg.request_repaint();
        });

        Self {
            mode: initial_mode,
            lang: load_lang(),
            hw_info,
            autostart: is_autostart_enabled(),
            auto_game: is_auto_game_enabled(),
            battery_limit: get_battery_limit_state(),
            kbd_backlight: get_kbd_backlight_state(),
            cpu_fan_pct: cpu_pct,
            gpu_fan_pct: gpu_pct,
            link_fans: true,
            fan_curve,
            status_msg: None,
            shared,
            visible_flag,
        }
    }
}

fn push_sample(q: &mut VecDeque<f64>, val: f64) {
    if q.len() >= HISTORY_LEN {
        q.pop_front();
    }
    q.push_back(val);
}

fn apply_crystal_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG_OBSIDIAN;
    visuals.window_fill = BG_OBSIDIAN;
    visuals.extreme_bg_color = Color32::from_rgb(5, 9, 16);
    visuals.faint_bg_color = CARD_CRYSTAL;

    visuals.widgets.noninteractive.bg_fill = CARD_CRYSTAL;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_ICE);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_CRYSTAL);

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(20, 34, 56);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_ICE);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_CRYSTAL);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(24, 52, 86);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, ACCENT_ICE);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, ACCENT_ICE);

    visuals.widgets.active.bg_fill = ACCENT_COBALT;
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, TEXT_ICE);
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, ACCENT_ICE);

    visuals.selection.bg_fill = ACCENT_COBALT;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT_ICE);

    ctx.set_visuals(visuals);
}

fn crystal_card(ui: &mut egui::Ui, active: bool, add_contents: impl FnOnce(&mut egui::Ui)) {
    let fill = if active { CARD_CRYSTAL_ACTIVE } else { CARD_CRYSTAL };
    let border = if active { ACCENT_ICE } else { BORDER_CRYSTAL };
    egui::Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .show(ui, add_contents);
}

/// Computes a smooth holographic iridescent color (Cyan -> Ice Blue -> Violet -> Magenta -> Cyan).
fn holo_color(phase: f32, alpha: u8) -> Color32 {
    let p = (phase.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
    let q = ((phase + 2.1).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
    let r = (56.0 + 185.0 * q) as u8;
    let g = (115.0 + 110.0 * p) as u8;
    let b = (225.0 + 30.0 * (1.0 - p * 0.4)) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// Renders animated diagonal crystal shards & cyan laser streaks rising continuously from bottom to top.
fn draw_crystal_backdrop(painter: &egui::Painter, rect: egui::Rect, t: f32, is_turbo: bool) {
    let w = rect.width();
    let h = rect.height();
    let origin = rect.min;

    // Diagonal dark crystal shards & cyan accent streaks rising from bottom to top
    let shard_specs: [(f32, f32, f32, f32, bool); 10] = [
        (0.08, 0.05, 22.0, 0.35, true),
        (0.18, 0.25, 44.0, 0.22, false),
        (0.32, 0.45, 18.0, 0.40, false),
        (0.45, 0.65, 36.0, 0.28, true),
        (0.58, 0.85, 52.0, 0.20, false),
        (0.66, 0.15, 16.0, 0.48, true),
        (0.75, 0.35, 30.0, 0.30, false),
        (0.84, 0.55, 14.0, 0.55, true),
        (0.92, 0.75, 28.0, 0.25, false),
        (0.24, 0.95, 12.0, 0.45, true),
    ];

    let margin = 150.0_f32;
    let span_y = h + margin * 2.0;

    for (idx, &(u, phase, thickness, speed, has_laser)) in shard_specs.iter().enumerate() {
        let progress = (phase + t * speed * 0.18).rem_euclid(1.0);
        let cx = origin.x + u * w;
        let cy = rect.bottom() + margin - progress * span_y;
        let len = 170.0 + (idx % 3) as f32 * 70.0;
        let dir = Vec2::new(0.72, -0.69).normalized();
        let perp = Vec2::new(-dir.y, dir.x);

        let p0 = Pos2::new(cx, cy) - dir * (len * 0.5) - perp * (thickness * 0.5);
        let p1 = Pos2::new(cx, cy) + dir * (len * 0.5) - perp * (thickness * 0.5);
        let p2 = Pos2::new(cx, cy) + dir * (len * 0.5) + perp * (thickness * 0.5);
        let p3 = Pos2::new(cx, cy) - dir * (len * 0.5) + perp * (thickness * 0.5);

        let shard_alpha = if idx % 2 == 0 { 22 } else { 14 };
        painter.add(egui::Shape::convex_polygon(
            vec![p0, p1, p2, p3],
            Color32::from_rgba_unmultiplied(32, 68, 114, shard_alpha),
            Stroke::NONE,
        ));

        if has_laser {
            let pulse = (t * 1.4 + idx as f32 * 0.9).sin() * 0.5 + 0.5;
            let laser_col = if is_turbo && idx % 3 == 0 {
                Color32::from_rgba_unmultiplied(251, 146, 60, (55.0 + 75.0 * pulse) as u8)
            } else {
                Color32::from_rgba_unmultiplied(56, 189, 248, (45.0 + 80.0 * pulse) as u8)
            };
            painter.line_segment([p0, p1], Stroke::new(1.2_f32, laser_col));
        }
    }
}

/// Renders a compact octagonal Nicontrol crystal badge in the header.
fn draw_header_emblem(ui: &mut egui::Ui, t: f32, mode: PowerMode) {
    let size = 34.0_f32;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let c = rect.center();
    let r = size * 0.44;
    let k = r * 0.42;

    let oct = vec![
        Pos2::new(c.x - k, c.y - r),
        Pos2::new(c.x + k, c.y - r),
        Pos2::new(c.x + r, c.y - k),
        Pos2::new(c.x + r, c.y + k),
        Pos2::new(c.x + k, c.y + r),
        Pos2::new(c.x - k, c.y + r),
        Pos2::new(c.x - r, c.y + k),
        Pos2::new(c.x - r, c.y - k),
    ];
    let rim = match mode {
        PowerMode::Turbo => ACCENT_TURBO_GLOW,
        PowerMode::Curve => Color32::from_rgb(45, 212, 191),
        PowerMode::Manual(_, _) => ACCENT_INDIGO,
        PowerMode::Auto => holo_color(t * 1.2, 230),
    };
    painter.add(egui::Shape::convex_polygon(
        oct,
        Color32::from_rgb(11, 24, 48),
        Stroke::new(1.5_f32, rim),
    ));

    let s = size * 0.025;
    let p = |dx: f32, dy: f32| Pos2::new(c.x + dx * s, c.y + dy * s);
    let left = vec![p(-9.0, -10.0), p(-4.5, -10.0), p(-4.5, 10.0), p(-9.0, 8.0)];
    let right = vec![p(4.5, -8.0), p(9.0, -10.0), p(9.0, 10.0), p(4.5, 10.0)];
    let diag = vec![p(-8.5, -10.0), p(-4.0, -10.0), p(8.5, 10.0), p(4.0, 10.0)];

    painter.add(egui::Shape::convex_polygon(left, ACCENT_ICE, Stroke::NONE));
    painter.add(egui::Shape::convex_polygon(right, ACCENT_INDIGO, Stroke::NONE));
    painter.add(egui::Shape::convex_polygon(diag, TEXT_ICE, Stroke::NONE));
}

/// Renders the centered holographic Acer Nitro "N" emblem with rotating crystal facets.
fn draw_holographic_nitro_emblem(ui: &mut egui::Ui, t: f32, mode: PowerMode) {
    let desired = Vec2::new(ui.available_width(), 54.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let c = rect.center();

    let speed = match mode {
        PowerMode::Turbo => 2.6_f32,
        PowerMode::Curve => 1.9_f32,
        PowerMode::Manual(_, _) => 1.7_f32,
        PowerMode::Auto => 1.2_f32,
    };

    // Flanking crystal energy lines
    let line_col = holo_color(t * speed, 90);
    let line_fade = Color32::from_rgba_unmultiplied(56, 189, 248, 22);
    painter.line_segment([Pos2::new(rect.left() + 8.0, c.y), Pos2::new(c.x - 52.0, c.y)], Stroke::new(1.0_f32, line_fade));
    painter.line_segment([Pos2::new(c.x - 110.0, c.y), Pos2::new(c.x - 42.0, c.y)], Stroke::new(1.5_f32, line_col));
    painter.line_segment([Pos2::new(c.x + 52.0, c.y), Pos2::new(rect.right() - 8.0, c.y)], Stroke::new(1.0_f32, line_fade));
    painter.line_segment([Pos2::new(c.x + 42.0, c.y), Pos2::new(c.x + 110.0, c.y)], Stroke::new(1.5_f32, line_col));

    // Rotating crystal diamond ring around the Nitro N
    for layer in 0..2 {
        let dir = if layer == 0 { 1.0_f32 } else { -1.0_f32 };
        let r = if layer == 0 { 24.0_f32 } else { 19.0_f32 };
        let ang_off = t * 0.7 * dir + layer as f32 * 0.78;
        let mut pts = [Pos2::ZERO; 4];
        for (i, pt) in pts.iter_mut().enumerate() {
            let a = ang_off + (i as f32 * TAU / 4.0);
            *pt = Pos2::new(c.x + a.cos() * r * 1.35, c.y + a.sin() * r * 0.85);
        }
        let ring_col = holo_color(t * speed + layer as f32 * 1.4, if layer == 0 { 110 } else { 75 });
        painter.add(egui::Shape::closed_line(pts.to_vec(), Stroke::new(1.1_f32, ring_col)));
    }

    // Soft holographic aura behind the N
    painter.circle_filled(c, 22.0_f32, holo_color(t * speed, 28));

    // Iconic sharp Acer Nitro "N" silhouette built from 3 convex crystal facets
    let s = 1.05_f32;
    let p = |dx: f32, dy: f32| Pos2::new(c.x + dx * s, c.y + dy * s);

    // Left vertical/angled spike
    let left_facet = vec![p(-13.0, -17.0), p(-6.0, -10.0), p(-6.0, 15.0), p(-12.0, 12.0)];
    // Center diagonal lightning slash
    let diag_facet = vec![p(-11.0, -14.0), p(-5.0, -14.0), p(11.0, 14.0), p(5.0, 14.0)];
    // Right vertical/angled spike
    let right_facet = vec![p(6.0, -15.0), p(12.0, -12.0), p(13.0, 17.0), p(6.0, 10.0)];

    let c1 = holo_color(t * speed, 240);
    let c2 = holo_color(t * speed + 1.15, 245);
    let c3 = holo_color(t * speed + 2.30, 240);
    let rim = Color32::from_rgba_unmultiplied(240, 249, 255, 190);

    painter.add(egui::Shape::convex_polygon(left_facet, c1, Stroke::new(0.8_f32, rim)));
    painter.add(egui::Shape::convex_polygon(right_facet, c3, Stroke::new(0.8_f32, rim)));
    painter.add(egui::Shape::convex_polygon(diag_facet, c2, Stroke::new(0.9_f32, rim)));
}

/// Custom crystal-styled button for mode selection.
fn crystal_action_button(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    accent: Color32,
    fill_active: Color32,
) -> egui::Response {
    let desired = Vec2::new(ui.available_width(), 32.0);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    let hovered = resp.hovered();

    let fill = if active {
        fill_active
    } else if hovered {
        Color32::from_rgba_unmultiplied(26, 54, 90, 235)
    } else {
        Color32::from_rgba_unmultiplied(16, 30, 50, 220)
    };

    let border = if active || hovered {
        accent
    } else {
        BORDER_CRYSTAL
    };

    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(7),
        fill,
        Stroke::new(if active { 1.4_f32 } else { 1.0_f32 }, border),
        egui::StrokeKind::Inside,
    );

    // Top crystal highlight bar
    if active || hovered {
        let bar = egui::Rect::from_min_max(
            Pos2::new(rect.left() + 10.0, rect.top() + 1.0),
            Pos2::new(rect.right() - 10.0, rect.top() + 3.0),
        );
        painter.rect_filled(bar, CornerRadius::same(2), accent);
    }

    let text_col = if active { TEXT_ICE } else if hovered { ACCENT_CYAN_SOFT } else { TEXT_ICE };
    let galley = painter.layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(13.0),
        text_col,
    );
    if active {
        let dot_r = 4.0_f32;
        let gap = 7.0_f32;
        let total_w = dot_r * 2.0 + gap + galley.size().x;
        let start_x = rect.center().x - total_w * 0.5;
        let dot_center = Pos2::new(start_x + dot_r, rect.center().y);
        let [r, g, b, _] = accent.to_array();
        painter.circle_filled(dot_center, dot_r + 2.5, Color32::from_rgba_unmultiplied(r, g, b, 70));
        painter.circle_filled(dot_center, dot_r, accent);
        painter.galley(
            Pos2::new(start_x + dot_r * 2.0 + gap, rect.center().y - galley.size().y * 0.5),
            galley,
            text_col,
        );
    } else {
        painter.galley(
            Pos2::new(
                rect.center().x - galley.size().x * 0.5,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            text_col,
        );
    }

    resp
}

struct ChartSeries<'a> {
    name: &'a str,
    unit: &'a str,
    values: &'a VecDeque<f64>,
    color: Color32,
    fill_gradient: bool,
    dashed: bool,
}

fn catmull_rom_smooth(raw: &[Pos2], y_min: f32, y_max: f32, subdivisions: usize) -> Vec<Pos2> {
    if raw.len() <= 2 || subdivisions <= 1 {
        return raw.to_vec();
    }
    let mut out = Vec::with_capacity((raw.len() - 1) * subdivisions + 1);
    for i in 0..raw.len() - 1 {
        let p0 = if i == 0 { raw[0] } else { raw[i - 1] };
        let p1 = raw[i];
        let p2 = raw[i + 1];
        let p3 = if i + 2 < raw.len() { raw[i + 2] } else { raw[raw.len() - 1] };

        for s in 0..subdivisions {
            let t = s as f32 / subdivisions as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let x = 0.5
                * ((2.0 * p1.x)
                    + (-p0.x + p2.x) * t
                    + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                    + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3);
            let y = 0.5
                * ((2.0 * p1.y)
                    + (-p0.y + p2.y) * t
                    + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                    + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3);
            out.push(Pos2::new(x, y.clamp(y_min, y_max)));
        }
    }
    if let Some(&last) = raw.last() {
        out.push(Pos2::new(last.x, last.y.clamp(y_min, y_max)));
    }
    out
}

fn paint_vertical_gradient(
    painter: &egui::Painter,
    pts: &[Pos2],
    baseline_y: f32,
    top_y: f32,
    color: Color32,
    max_alpha: u8,
) {
    if pts.len() < 2 {
        return;
    }
    let span = (baseline_y - top_y).max(1.0);
    let mut mesh = egui::Mesh::default();
    let [r, g, b, _] = color.to_array();

    for (i, &p) in pts.iter().enumerate() {
        let ratio = ((baseline_y - p.y) / span).clamp(0.0, 1.0);
        let alpha = (max_alpha as f32 * (0.35 + 0.65 * ratio)).round() as u8;
        let top_col = Color32::from_rgba_unmultiplied(r, g, b, alpha);
        let bot_col = Color32::from_rgba_unmultiplied(r, g, b, 0);

        mesh.vertices.push(egui::epaint::Vertex {
            pos: p,
            uv: egui::epaint::WHITE_UV,
            color: top_col,
        });
        mesh.vertices.push(egui::epaint::Vertex {
            pos: Pos2::new(p.x, baseline_y),
            uv: egui::epaint::WHITE_UV,
            color: bot_col,
        });

        if i > 0 {
            let base = (i as u32 - 1) * 2;
            mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
            mesh.indices.extend_from_slice(&[base + 1, base + 3, base + 2]);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn paint_glow_line(
    painter: &egui::Painter,
    pts: &[Pos2],
    color: Color32,
    width: f32,
    glow: bool,
    dashed: bool,
) {
    if pts.len() < 2 {
        return;
    }
    let [r, g, b, _] = color.to_array();

    if glow {
        painter.add(egui::Shape::line(
            pts.to_vec(),
            Stroke::new(width + 3.0, Color32::from_rgba_unmultiplied(r, g, b, 18)),
        ));
        painter.add(egui::Shape::line(
            pts.to_vec(),
            Stroke::new(width + 1.4, Color32::from_rgba_unmultiplied(r, g, b, 42)),
        ));
    }

    if dashed {
        painter.add(egui::Shape::dashed_line(
            pts,
            Stroke::new(width, color),
            4.5,
            3.5,
        ));
    } else {
        painter.add(egui::Shape::line(pts.to_vec(), Stroke::new(width, color)));
    }
}

fn draw_smooth_glow_chart(ui: &mut egui::Ui, height: f32, max_y: f64, series: &[ChartSeries<'_>]) {
    let desired = Vec2::new(ui.available_width(), height);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // Chart container
    painter.rect(
        rect,
        CornerRadius::same(8),
        Color32::from_rgba_premultiplied(8, 14, 24, 235),
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(56, 189, 248, 32)),
        egui::StrokeKind::Inside,
    );

    let plot = egui::Rect::from_min_max(
        Pos2::new(rect.left() + 30.0, rect.top() + 24.0),
        Pos2::new(rect.right() - 10.0, rect.bottom() - 10.0),
    );
    if plot.width() <= 10.0 || plot.height() <= 10.0 {
        return;
    }

    // Subtle horizontal grid + Y labels
    for pct in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
        let y = plot.bottom() - pct * plot.height();
        let grid_col = if pct == 0.0 {
            Color32::from_rgba_unmultiplied(148, 163, 184, 42)
        } else {
            Color32::from_rgba_unmultiplied(148, 163, 184, 22)
        };
        painter.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], Stroke::new(1.0_f32, grid_col));

        if pct == 0.0 || pct == 0.5 || pct == 1.0 {
            let val = (max_y as f32 * pct).round() as i32;
            painter.text(
                Pos2::new(rect.left() + 25.0, y),
                egui::Align2::RIGHT_CENTER,
                format!("{val}"),
                egui::FontId::proportional(10.0),
                TEXT_MUTED,
            );
        }
    }

    let map_series = |values: &VecDeque<f64>| -> Vec<Pos2> {
        let n = values.len();
        if n == 0 {
            return Vec::new();
        }
        let denom = (n.saturating_sub(1)).max(1) as f32;
        values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let x = plot.left() + (i as f32 / denom) * plot.width();
                let frac = (*v / max_y).clamp(0.0, 1.0) as f32;
                let y = plot.bottom() - frac * plot.height();
                Pos2::new(x, y)
            })
            .collect()
    };

    // Draw fills first, then lines
    let mut smoothed_all: Vec<Vec<Pos2>> = Vec::with_capacity(series.len());
    let mut raw_all: Vec<Vec<Pos2>> = Vec::with_capacity(series.len());
    for s in series {
        let raw = map_series(s.values);
        let smooth = catmull_rom_smooth(&raw, plot.top(), plot.bottom(), 4);
        if s.fill_gradient {
            paint_vertical_gradient(&painter, &smooth, plot.bottom(), plot.top(), s.color, 76);
        }
        raw_all.push(raw);
        smoothed_all.push(smooth);
    }
    for (s, smooth) in series.iter().zip(smoothed_all.iter()) {
        paint_glow_line(
            &painter,
            smooth,
            s.color,
            if s.fill_gradient { 1.6 } else { 1.25 },
            s.fill_gradient,
            s.dashed,
        );
    }

    // Compact legend at top-left
    let mut lx = plot.left();
    let ly = rect.top() + 12.0;
    for s in series {
        painter.circle_filled(Pos2::new(lx + 4.0, ly), 3.5, s.color);
        let galley = painter.layout_no_wrap(
            s.name.to_string(),
            egui::FontId::proportional(10.5),
            TEXT_ICE,
        );
        let text_w = galley.size().x;
        painter.galley(Pos2::new(lx + 11.0, ly - galley.size().y * 0.5), galley, TEXT_ICE);
        lx += 20.0 + text_w;
    }

    // Interactive hover crosshair
    if let Some(hover) = resp.hover_pos() {
        if plot.contains(hover) {
            if let Some(first) = series.first() {
                let n = first.values.len();
                if n > 1 {
                    let frac = ((hover.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
                    let idx = ((frac * (n - 1) as f32).round() as usize).min(n - 1);
                    let cx = plot.left() + (idx as f32 / (n - 1) as f32) * plot.width();
                    painter.line_segment(
                        [Pos2::new(cx, plot.top()), Pos2::new(cx, plot.bottom())],
                        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(226, 232, 240, 80)),
                    );

                    let mut parts = Vec::with_capacity(series.len());
                    for (s, raw) in series.iter().zip(raw_all.iter()) {
                        if let Some(&pt) = raw.get(idx) {
                            painter.circle_filled(pt, 4.0, s.color);
                            painter.circle_stroke(pt, 4.5, Stroke::new(1.2_f32, BG_OBSIDIAN));
                        }
                        if let Some(v) = s.values.get(idx) {
                            parts.push(format!("{:.0}{}", v, s.unit));
                        }
                    }

                    if !parts.is_empty() {
                        let label = parts.join("  •  ");
                        let galley = painter.layout_no_wrap(
                            label,
                            egui::FontId::proportional(10.5),
                            TEXT_ICE,
                        );
                        let pill_w = galley.size().x + 12.0;
                        let pill_h = galley.size().y + 6.0;
                        let px = (cx + 8.0).min(plot.right() - pill_w - 4.0).max(plot.left() + 4.0);
                        let py = plot.top() + 2.0;
                        let pill = egui::Rect::from_min_size(Pos2::new(px, py), Vec2::new(pill_w, pill_h));
                        painter.rect(
                            pill,
                            CornerRadius::same(5),
                            Color32::from_rgba_premultiplied(15, 23, 42, 230),
                            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(56, 189, 248, 80)),
                            egui::StrokeKind::Inside,
                        );
                        painter.galley(Pos2::new(px + 6.0, py + 3.0), galley, TEXT_ICE);
                    }
                }
            }
        }
    }
}

impl eframe::App for NitroApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let is_minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        let screen_rect = ctx.screen_rect();
        let is_visible = !is_minimized && screen_rect.width() > 10.0 && screen_rect.height() > 10.0;
        self.visible_flag.store(is_visible, Ordering::Relaxed);
        let t = ctx.input(|i| i.time) as f32;

        if is_visible {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else {
            // Minimized / hidden: skip heavy UI & motion graphics rendering
            return;
        }

        let (synced_mode, tel, update_info, cpu_hist, gpu_hist, power_hist, cpu_t_hist, gpu_t_hist) = {
            let d = self.shared.lock().unwrap();
            (
                d.synced_mode,
                d.telemetry.clone(),
                d.update_info.clone(),
                d.cpu_usage_hist.clone(),
                d.gpu_usage_hist.clone(),
                d.gpu_power_hist.clone(),
                d.cpu_temp_hist.clone(),
                d.gpu_temp_hist.clone(),
            )
        };
        self.mode = synced_mode;
        let s = strings(self.lang);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG_OBSIDIAN).inner_margin(egui::Margin::same(18)))
            .show(ctx, |ui| {
                // Animated Crystal Shards & Laser Streaks (only when visible)
                draw_crystal_backdrop(ui.painter(), ui.max_rect(), t, matches!(self.mode, PowerMode::Turbo));

                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Header
                    ui.horizontal(|ui| {
                        draw_header_emblem(ui, t, self.mode);
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("NICONTROL")
                                        .size(22.0)
                                        .strong()
                                        .color(ACCENT_ICE),
                                );
                                ui.add_space(6.0);
                                egui::Frame::NONE
                                    .fill(Color32::from_rgb(18, 40, 68))
                                    .stroke(Stroke::new(1.0_f32, ACCENT_COBALT))
                                    .corner_radius(CornerRadius::same(5))
                                    .inner_margin(egui::Margin::symmetric(8, 2))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(&self.hw_info.laptop_model)
                                                .size(11.0)
                                                .strong()
                                                .color(ACCENT_CYAN_SOFT),
                                        );
                                    });
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(format!("v{CURRENT_VERSION}"))
                                        .size(11.0)
                                        .color(TEXT_MUTED),
                                );
                            });
                            ui.label(
                                RichText::new(format!("{}: {}", s.active_profile, self.mode.label(self.lang)))
                                    .size(12.5)
                                    .color(TEXT_MUTED),
                            );
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let lang_btn_label = match self.lang {
                                Lang::En => "EN",
                                Lang::PtBr => "PT-BR",
                            };
                            if ui.button(lang_btn_label).clicked() {
                                self.lang = match self.lang {
                                    Lang::En => Lang::PtBr,
                                    Lang::PtBr => Lang::En,
                                };
                                save_lang(self.lang);
                            }

                            ui.add_space(8.0);

                            let mut auto_checked = self.autostart;
                            if ui
                                .checkbox(
                                    &mut auto_checked,
                                    RichText::new(s.start_in_tray)
                                        .size(12.5)
                                        .color(TEXT_ICE),
                                )
                                .changed()
                            {
                                match set_autostart(auto_checked) {
                                    Ok(()) => {
                                        self.autostart = auto_checked;
                                        self.status_msg = Some(if auto_checked {
                                            s.msg_autostart_on.to_string()
                                        } else {
                                            s.msg_autostart_off.to_string()
                                        });
                                    }
                                    Err(e) => self.status_msg = Some(e),
                                }
                            }
                        });
                    });

                    ui.add_space(10.0);

                    if let Some(upd) = &update_info {
                        egui::Frame::NONE
                            .fill(Color32::from_rgba_premultiplied(14, 52, 82, 235))
                            .stroke(Stroke::new(1.0_f32, ACCENT_ICE))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(egui::Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{}: {}",
                                            s.update_available, upd.latest_version
                                        ))
                                        .size(12.5)
                                        .strong()
                                        .color(ACCENT_CYAN_SOFT),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.button(s.update_download).clicked() {
                                                open_url(&upd.release_url);
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(10.0);
                    }

                    // Power Mode Cards (Crystal Styled)
                    ui.columns(4, |cols| {
                        let is_auto = matches!(self.mode, PowerMode::Auto);
                        let is_turbo = matches!(self.mode, PowerMode::Turbo);
                        let is_manual = matches!(self.mode, PowerMode::Manual(_, _));
                        let is_curve = matches!(self.mode, PowerMode::Curve);

                        crystal_card(&mut cols[0], is_auto, |ui| {
                            ui.set_min_height(84.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(s.card_auto_title).size(12.5).strong().color(
                                    if is_auto { ACCENT_ICE } else { TEXT_ICE },
                                ));
                            });
                            ui.label(
                                RichText::new(s.card_auto_desc)
                                    .size(11.0)
                                    .color(TEXT_MUTED),
                            );
                            ui.add_space(8.0);
                            if crystal_action_button(
                                ui,
                                if is_auto { s.btn_auto_active } else { s.btn_auto_activate },
                                is_auto,
                                ACCENT_ICE,
                                Color32::from_rgba_unmultiplied(2, 96, 150, 235),
                            )
                            .clicked()
                            {
                                if let Err(e) = apply_auto() {
                                    self.status_msg = Some(e);
                                } else {
                                    self.mode = PowerMode::Auto;
                                    self.status_msg = Some(s.msg_auto_on.to_string());
                                }
                            }
                        });

                        crystal_card(&mut cols[1], is_turbo, |ui| {
                            ui.set_min_height(84.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(s.card_turbo_title).size(12.5).strong().color(
                                    if is_turbo { ACCENT_TURBO_GLOW } else { TEXT_ICE },
                                ));
                            });
                            ui.label(
                                RichText::new(s.card_turbo_desc)
                                    .size(11.0)
                                    .color(TEXT_MUTED),
                            );
                            ui.add_space(8.0);
                            if crystal_action_button(
                                ui,
                                if is_turbo { s.btn_turbo_active } else { s.btn_turbo_activate },
                                is_turbo,
                                ACCENT_TURBO_GLOW,
                                Color32::from_rgba_unmultiplied(185, 62, 12, 235),
                            )
                            .clicked()
                            {
                                if let Err(e) = apply_turbo() {
                                    self.status_msg = Some(e);
                                } else {
                                    self.mode = PowerMode::Turbo;
                                    self.cpu_fan_pct = 100;
                                    self.gpu_fan_pct = 100;
                                    self.status_msg = Some(s.msg_turbo_on.to_string());
                                }
                            }
                        });

                        crystal_card(&mut cols[2], is_manual, |ui| {
                            ui.set_min_height(84.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(s.card_manual_title).size(12.5).strong().color(
                                    if is_manual { ACCENT_INDIGO } else { TEXT_ICE },
                                ));
                            });
                            ui.label(
                                RichText::new(s.card_manual_desc)
                                    .size(11.0)
                                    .color(TEXT_MUTED),
                            );
                            ui.add_space(8.0);
                            if crystal_action_button(
                                ui,
                                if is_manual { s.btn_manual_active } else { s.btn_manual_apply },
                                is_manual,
                                ACCENT_INDIGO,
                                Color32::from_rgba_unmultiplied(67, 56, 202, 235),
                            )
                            .clicked()
                            {
                                let f1 = pct_to_raw(self.cpu_fan_pct);
                                let f2 = pct_to_raw(self.gpu_fan_pct);
                                if let Err(e) = apply_manual_fans(f1, f2) {
                                    self.status_msg = Some(e);
                                } else {
                                    self.mode = PowerMode::Manual(f1, f2);
                                    self.status_msg = Some(s.msg_manual_applied.to_string());
                                }
                            }
                        });

                        let accent_teal = Color32::from_rgb(45, 212, 191);
                        crystal_card(&mut cols[3], is_curve, |ui| {
                            ui.set_min_height(84.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(s.card_curve_title).size(12.5).strong().color(
                                    if is_curve { accent_teal } else { TEXT_ICE },
                                ));
                            });
                            ui.label(
                                RichText::new(s.card_curve_desc)
                                    .size(11.0)
                                    .color(TEXT_MUTED),
                            );
                            ui.add_space(8.0);
                            if crystal_action_button(
                                ui,
                                if is_curve { s.btn_curve_active } else { s.btn_curve_activate },
                                is_curve,
                                accent_teal,
                                Color32::from_rgba_unmultiplied(15, 76, 92, 235),
                            )
                            .clicked()
                            {
                                save_curve(&self.fan_curve);
                                if let Err(e) = apply_curve_mode() {
                                    self.status_msg = Some(e);
                                } else {
                                    self.mode = PowerMode::Curve;
                                    self.status_msg = Some(s.msg_curve_on.to_string());
                                }
                            }
                        });
                    });

                    ui.add_space(6.0);

                    // Center Holographic Nitro "N" Emblem
                    draw_holographic_nitro_emblem(ui, t, self.mode);

                    ui.add_space(6.0);

                    // Telemetry Summary Cards: 1 row, grouped as [CPU | RAM] and [GPU | VRAM]
                    ui.columns(2, |outer| {
                        outer[0].scope(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.columns(2, |cols| {
                        crystal_card(&mut cols[0], false, |ui| {
                            ui.set_min_height(68.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("CPU").size(11.5).strong().color(ACCENT_ICE));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&self.hw_info.cpu_model)
                                            .size(11.0)
                                            .color(TEXT_MUTED),
                                    )
                                    .truncate(),
                                );
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                let temp = tel
                                    .cpu_temp_c
                                    .map(|t| format!("{t}°C"))
                                    .unwrap_or_else(|| "--°C".into());
                                let usage = tel
                                    .cpu_usage_pct
                                    .map(|u| format!("{u:.0}%"))
                                    .unwrap_or_else(|| "--%".into());
                                let freq_str = tel
                                    .cpu_freq_mhz
                                    .map(|f| format!("{f} MHz"))
                                    .unwrap_or_else(|| "-- MHz".to_string());
                                ui.label(RichText::new(temp).size(20.0).strong().color(TEXT_ICE));
                                ui.add_space(6.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(format!("{}: {usage}", s.cpu_usage))
                                            .size(11.5)
                                            .color(ACCENT_CYAN_SOFT),
                                    );
                                    ui.label(
                                        RichText::new(freq_str)
                                            .size(10.5)
                                            .color(TEXT_MUTED),
                                    );
                                });
                            });
                        });

                        crystal_card(&mut cols[1], false, |ui| {
                            ui.set_min_height(68.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(s.ram_label).size(11.5).strong().color(ACCENT_ICE));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new("Memory")
                                            .size(11.0)
                                            .color(TEXT_MUTED),
                                    )
                                    .truncate(),
                                );
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                let (pct_str, used_str, free_str) = match (tel.ram_used_mb, tel.ram_total_mb) {
                                    (Some(u), Some(t)) if t > 0 => {
                                        let pct = (u as f32 / t as f32) * 100.0;
                                        let free = t.saturating_sub(u);
                                        (
                                            format!("{pct:.0}%"),
                                            format!("{:.1}/{:.1} GB", u as f32 / 1024.0, t as f32 / 1024.0),
                                            format!("Free {:.1} GB", free as f32 / 1024.0),
                                        )
                                    }
                                    _ => ("--%".to_string(), "-- GB".to_string(), "Free --".to_string()),
                                };
                                ui.label(RichText::new(pct_str).size(20.0).strong().color(TEXT_ICE));
                                ui.add_space(6.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(used_str)
                                            .size(11.5)
                                            .color(ACCENT_CYAN_SOFT),
                                    );
                                    ui.label(
                                        RichText::new(free_str)
                                            .size(10.5)
                                            .color(TEXT_MUTED),
                                    );
                                });
                            });
                        });
                            });
                        });

                        outer[1].scope(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.columns(2, |cols| {
                        crystal_card(&mut cols[0], false, |ui| {
                            ui.set_min_height(68.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("GPU").size(11.5).strong().color(ACCENT_ICE));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&self.hw_info.gpu_model)
                                            .size(11.0)
                                            .color(TEXT_MUTED),
                                    )
                                    .truncate(),
                                );
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                let temp = tel
                                    .gpu_temp_c
                                    .map(|t| format!("{t}°C"))
                                    .unwrap_or_else(|| "--°C".into());
                                let power = tel
                                    .gpu_power_w
                                    .map(|w| format!("{w:.0}W"))
                                    .unwrap_or_else(|| "--W".into());
                                let usage = tel
                                    .gpu_usage_pct
                                    .map(|u| format!("{u:.0}%"))
                                    .unwrap_or_else(|| "--%".into());
                                let clock = tel
                                    .gpu_clock_mhz
                                    .map(|c| format!("{c} MHz"))
                                    .unwrap_or_else(|| "-- MHz".into());

                                ui.label(RichText::new(temp).size(20.0).strong().color(TEXT_ICE));
                                ui.add_space(6.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(format!("{power} | {usage}"))
                                            .size(11.5)
                                            .color(ACCENT_CYAN_SOFT),
                                    );
                                    ui.label(
                                        RichText::new(clock)
                                            .size(10.5)
                                            .color(TEXT_MUTED),
                                    );
                                });
                            });
                        });

                        crystal_card(&mut cols[1], false, |ui| {
                            ui.set_min_height(68.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("VRAM").size(11.5).strong().color(ACCENT_ICE));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new("GPU Mem")
                                            .size(11.0)
                                            .color(TEXT_MUTED),
                                    )
                                    .truncate(),
                                );
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                let (pct_str, used_str, free_str) = match (tel.gpu_vram_mb, tel.gpu_vram_total_mb) {
                                    (Some(used), Some(total)) if total > 0 => {
                                        let pct = (used as f32 / total as f32) * 100.0;
                                        let free = total.saturating_sub(used);
                                        (
                                            format!("{pct:.0}%"),
                                            format!("{used}/{total} MB"),
                                            format!("Free {free} MB"),
                                        )
                                    }
                                    (Some(used), _) => ("--%".to_string(), format!("{used} MB"), "Free --".to_string()),
                                    _ => ("--%".to_string(), "-- MB".to_string(), "Free --".to_string()),
                                };
                                ui.label(RichText::new(pct_str).size(20.0).strong().color(TEXT_ICE));
                                ui.add_space(6.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(used_str)
                                            .size(11.5)
                                            .color(ACCENT_CYAN_SOFT),
                                    );
                                    ui.label(
                                        RichText::new(free_str)
                                            .size(10.5)
                                            .color(TEXT_MUTED),
                                    );
                                });
                            });
                        });
                            });
                        });
                    });

                    ui.add_space(12.0);

                    // Real-time Charts (Separated CPU & GPU)
                    let primary_col = if self.mode == PowerMode::Turbo { ACCENT_TURBO_GLOW } else { ACCENT_ICE };
                    ui.columns(2, |cols| {
                        crystal_card(&mut cols[0], false, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(s.cpu_chart_title)
                                        .size(12.5)
                                        .strong()
                                        .color(ACCENT_ICE),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let t = tel
                                        .cpu_temp_c
                                        .map(|v| format!("{v}°C"))
                                        .unwrap_or_else(|| "--".to_string());
                                    let u = tel
                                        .cpu_usage_pct
                                        .map(|v| format!("{v:.0}%"))
                                        .unwrap_or_else(|| "--".to_string());
                                    ui.label(
                                        RichText::new(format!("{u} | {t}"))
                                            .size(11.5)
                                            .color(ACCENT_CYAN_SOFT),
                                    );
                                });
                            });
                            ui.add_space(4.0);

                            draw_smooth_glow_chart(
                                ui,
                                165.0,
                                100.0,
                                &[
                                    ChartSeries {
                                        name: "CPU %",
                                        unit: "%",
                                        values: &cpu_hist,
                                        color: primary_col,
                                        fill_gradient: true,
                                        dashed: false,
                                    },
                                    ChartSeries {
                                        name: "CPU °C",
                                        unit: "°C",
                                        values: &cpu_t_hist,
                                        color: ACCENT_INDIGO,
                                        fill_gradient: false,
                                        dashed: true,
                                    },
                                ],
                            );
                        });

                        crystal_card(&mut cols[1], false, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(s.gpu_chart_title)
                                        .size(12.5)
                                        .strong()
                                        .color(ACCENT_ICE),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let t = tel
                                        .gpu_temp_c
                                        .map(|v| format!("{v}°C"))
                                        .unwrap_or_else(|| "--".to_string());
                                    let w = tel
                                        .gpu_power_w
                                        .map(|v| format!("{v:.0}W"))
                                        .unwrap_or_else(|| "--".to_string());
                                    let u = tel
                                        .gpu_usage_pct
                                        .map(|v| format!("{v:.0}%"))
                                        .unwrap_or_else(|| "--".to_string());
                                    ui.label(
                                        RichText::new(format!("{u} | {w} | {t}"))
                                            .size(11.5)
                                            .color(ACCENT_CYAN_SOFT),
                                    );
                                });
                            });
                            ui.add_space(4.0);

                            draw_smooth_glow_chart(
                                ui,
                                165.0,
                                100.0,
                                &[
                                    ChartSeries {
                                        name: "GPU %",
                                        unit: "%",
                                        values: &gpu_hist,
                                        color: primary_col,
                                        fill_gradient: true,
                                        dashed: false,
                                    },
                                    ChartSeries {
                                        name: "GPU W",
                                        unit: "W",
                                        values: &power_hist,
                                        color: Color32::from_rgb(251, 191, 36),
                                        fill_gradient: false,
                                        dashed: false,
                                    },
                                    ChartSeries {
                                        name: "GPU °C",
                                        unit: "°C",
                                        values: &gpu_t_hist,
                                        color: Color32::from_rgb(52, 211, 153),
                                        fill_gradient: false,
                                        dashed: true,
                                    },
                                ],
                            );
                        });
                    });

                    ui.add_space(12.0);

                    // Fine-grained Fan Controls
                    crystal_card(ui, false, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(s.fine_fan_title)
                                    .size(13.5)
                                    .strong()
                                    .color(ACCENT_ICE),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.checkbox(&mut self.link_fans, s.sync_fans);
                            });
                        });
                        ui.add_space(8.0);

                        let mut cpu_val = self.cpu_fan_pct;
                        let mut gpu_val = self.gpu_fan_pct;

                        ui.horizontal(|ui| {
                            ui.label(RichText::new(s.cpu_fan_label).size(12.5).color(TEXT_ICE));
                            let slider = egui::Slider::new(&mut cpu_val, 0..=100)
                                .suffix("%")
                                .step_by(5.0);
                            if ui.add_sized([280.0, 20.0], slider).changed() {
                                self.cpu_fan_pct = cpu_val;
                                if self.link_fans {
                                    self.gpu_fan_pct = cpu_val;
                                    gpu_val = cpu_val;
                                }
                            }
                            let raw = pct_to_raw(self.cpu_fan_pct);
                            let rpm_str = tel
                                .fans
                                .first()
                                .and_then(|f| f.rpm)
                                .map(|r| format!("  •  {r} RPM"))
                                .unwrap_or_default();
                            ui.label(RichText::new(format!("({raw} / 2560{rpm_str})")).size(11.5).color(TEXT_MUTED));
                        });

                        ui.add_space(4.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new(s.gpu_fan_label).size(12.5).color(TEXT_ICE));
                            let slider = egui::Slider::new(&mut gpu_val, 0..=100)
                                .suffix("%")
                                .step_by(5.0);
                            if ui.add_sized([280.0, 20.0], slider).changed() {
                                self.gpu_fan_pct = gpu_val;
                                if self.link_fans {
                                    self.cpu_fan_pct = gpu_val;
                                }
                            }
                            let raw = pct_to_raw(self.gpu_fan_pct);
                            let rpm_str = tel
                                .fans
                                .get(1)
                                .and_then(|f| f.rpm)
                                .map(|r| format!("  •  {r} RPM"))
                                .unwrap_or_default();
                            ui.label(RichText::new(format!("({raw} / 2560{rpm_str})")).size(11.5).color(TEXT_MUTED));
                        });

                        if self.cpu_fan_pct < 20 || self.gpu_fan_pct < 20 {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(s.low_fan_warning)
                                    .size(11.5)
                                    .color(ACCENT_TURBO_GLOW),
                            );
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button(s.apply_speed).clicked() {
                                let f1 = pct_to_raw(self.cpu_fan_pct);
                                let f2 = pct_to_raw(self.gpu_fan_pct);
                                if let Err(e) = apply_manual_fans(f1, f2) {
                                    self.status_msg = Some(e);
                                } else {
                                    self.mode = PowerMode::Manual(f1, f2);
                                    self.status_msg = Some(s.msg_fans_adjusted.to_string());
                                }
                            }
                            if ui.button(s.restore_auto).clicked() {
                                if let Err(e) = apply_auto() {
                                    self.status_msg = Some(e);
                                } else {
                                    self.mode = PowerMode::Auto;
                                    self.status_msg = Some(s.msg_fans_auto.to_string());
                                }
                            }
                            if let Some(msg) = &self.status_msg {
                                ui.add_space(10.0);
                                ui.label(RichText::new(msg).size(12.0).color(ACCENT_CYAN_SOFT));
                            }
                        });
                    });

                    ui.add_space(10.0);

                    // Custom Temperature Fan Curve Card
                    crystal_card(ui, matches!(self.mode, PowerMode::Curve), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(s.curve_editor_title)
                                    .size(13.5)
                                    .strong()
                                    .color(Color32::from_rgb(45, 212, 191)),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(s.curve_save_apply).clicked() {
                                    save_curve(&self.fan_curve);
                                    if let Err(e) = apply_curve_mode() {
                                        self.status_msg = Some(e);
                                    } else {
                                        self.mode = PowerMode::Curve;
                                        self.status_msg = Some(s.msg_curve_on.to_string());
                                    }
                                }
                            });
                        });
                        ui.add_space(6.0);
                        ui.columns(4, |cols| {
                            for (idx, pt) in self.fan_curve.points.iter_mut().enumerate() {
                                if idx < 4 {
                                    cols[idx].label(
                                        RichText::new(format!("{} {}", s.curve_point_prefix, idx + 1))
                                            .size(11.5)
                                            .strong()
                                            .color(TEXT_ICE),
                                    );
                                    cols[idx].add(
                                        egui::Slider::new(&mut pt.temp_c, 35..=95)
                                            .suffix(" °C"),
                                    );
                                    cols[idx].add(
                                        egui::Slider::new(&mut pt.fan_pct, 20..=100)
                                            .suffix("%")
                                            .step_by(5.0),
                                    );
                                }
                            }
                        });
                    });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let mut auto_game = self.auto_game;
                        if ui
                            .checkbox(
                                &mut auto_game,
                                RichText::new(s.auto_game_mode).size(12.0).color(TEXT_ICE),
                            )
                            .changed()
                        {
                            self.auto_game = auto_game;
                            set_auto_game_enabled(auto_game);
                        }

                        if self.battery_limit.supported {
                            ui.add_space(16.0);
                            let mut bat_80 = self.battery_limit.enabled_80;
                            if ui
                                .checkbox(
                                    &mut bat_80,
                                    RichText::new(s.battery_limit_80).size(12.0).color(TEXT_ICE),
                                )
                                .changed()
                            {
                                match set_battery_limit_80(bat_80) {
                                    Ok(()) => self.battery_limit.enabled_80 = bat_80,
                                    Err(e) => self.status_msg = Some(e),
                                }
                            }
                        }

                        if self.kbd_backlight.supported {
                            ui.add_space(16.0);
                            ui.label(RichText::new(s.kbd_backlight).size(12.0).color(TEXT_ICE));
                            let mut br = self.kbd_backlight.brightness;
                            let max_br = self.kbd_backlight.max_brightness;
                            if ui
                                .add(egui::Slider::new(&mut br, 0..=max_br))
                                .changed()
                            {
                                if let Err(e) = set_kbd_brightness(br) {
                                    self.status_msg = Some(e);
                                } else {
                                    self.kbd_backlight.brightness = br;
                                }
                            }
                        }
                    });

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(s.footer_disclaimer)
                            .size(10.5)
                            .color(TEXT_MUTED),
                    );
                });
            });
    }
}

fn gui_pid_path() -> PathBuf {
    state_file_path()
        .parent()
        .map(|p| p.join("gui.pid"))
        .unwrap_or_else(|| PathBuf::from("/tmp/nicontrol-gui.pid"))
}

fn is_gui_already_running() -> bool {
    let Ok(pid_str) = fs::read_to_string(gui_pid_path()) else {
        return false;
    };
    let Ok(pid) = pid_str.trim().parse::<u32>() else {
        return false;
    };
    if pid == std::process::id() {
        return false;
    }
    let Ok(cmdline) = fs::read(format!("/proc/{pid}/cmdline")) else {
        return false;
    };
    let text = String::from_utf8_lossy(&cmdline);
    text.contains("nicontrol") && !text.contains("--tray")
}

fn build_window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/nicontrol.png"))
        .unwrap_or_default()
}

pub fn run_gui() {
    if is_gui_already_running() {
        eprintln!("Nicontrol GUI is already running.");
        return;
    }

    let pid_path = gui_pid_path();
    if let Some(parent) = pid_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&pid_path, std::process::id().to_string());

    crate::tray::ensure_tray_running();

    let visible_flag = Arc::new(AtomicBool::new(true));
    let vis_clone = Arc::clone(&visible_flag);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(Vec2::new(820.0, 690.0))
            .with_min_inner_size(Vec2::new(680.0, 560.0))
            .with_icon(build_window_icon())
            .with_title("Nicontrol"),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "Nicontrol",
        options,
        Box::new(move |cc| Ok(Box::new(NitroApp::new(cc, vis_clone)))),
    ) {
        eprintln!("Erro ao iniciar interface gráfica do Nicontrol: {e}");
    }

    let _ = fs::remove_file(pid_path);
}
