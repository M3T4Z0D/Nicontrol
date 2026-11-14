use crate::hardware::{
    apply_auto, apply_curve_mode, apply_manual_fans, apply_turbo, detect_hardware_info,
    is_autostart_enabled, load_curve, load_saved_mode, pct_to_raw, raw_to_pct, read_telemetry,
    save_curve, set_autostart, state_file_path, CpuSampler, FanCurve, HardwareInfo, PowerMode,
    Telemetry,
};
use crate::i18n::{load_lang, save_lang, strings, Lang};
use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, RichText, Stroke, Vec2};
use egui_plot::{Legend, Line, Plot, PlotPoints};
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
            cpu_usage_hist: VecDeque::from(vec![init_cpu_u; HISTORY_LEN]),
            gpu_usage_hist: VecDeque::from(vec![init_gpu_u; HISTORY_LEN]),
            gpu_power_hist: VecDeque::from(vec![init_gpu_p; HISTORY_LEN]),
            cpu_temp_hist: VecDeque::from(vec![init_cpu_t; HISTORY_LEN]),
            gpu_temp_hist: VecDeque::from(vec![init_gpu_t; HISTORY_LEN]),
        }));

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
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0),
        text_col,
    );

    resp
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

        let (synced_mode, tel, cpu_hist, gpu_hist, power_hist, cpu_t_hist, gpu_t_hist) = {
            let d = self.shared.lock().unwrap();
            (
                d.synced_mode,
                d.telemetry.clone(),
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
                            });
                            ui.label(
                                RichText::new(format!("{}: {}", s.active_profile, self.mode.label(self.lang)))
                                    .size(12.5)
                                    .color(TEXT_MUTED),
                            );
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let lang_btn_label = match self.lang {
                                Lang::En => "🌐 EN",
                                Lang::PtBr => "🌐 PT-BR",
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

                    ui.add_space(14.0);

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

                    // Telemetry Summary Cards
                    ui.columns(2, |cols| {
                        crystal_card(&mut cols[0], false, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("CPU").size(12.0).strong().color(ACCENT_ICE));
                                ui.label(
                                    RichText::new(&self.hw_info.cpu_model)
                                        .size(12.0)
                                        .color(TEXT_MUTED),
                                );
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                let temp = tel
                                    .cpu_temp_c
                                    .map(|t| format!("{t} °C"))
                                    .unwrap_or_else(|| "-- °C".into());
                                let usage = tel
                                    .cpu_usage_pct
                                    .map(|u| format!("{u:.1}%"))
                                    .unwrap_or_else(|| "--%".into());
                                ui.label(RichText::new(temp).size(24.0).strong().color(TEXT_ICE));
                                ui.add_space(16.0);
                                ui.label(
                                    RichText::new(format!("{}: {usage}", s.cpu_usage))
                                        .size(15.0)
                                        .color(ACCENT_CYAN_SOFT),
                                );
                            });
                        });

                        crystal_card(&mut cols[1], false, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("GPU").size(12.0).strong().color(ACCENT_ICE));
                                ui.label(
                                    RichText::new(&self.hw_info.gpu_model)
                                        .size(12.0)
                                        .color(TEXT_MUTED),
                                );
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                let temp = tel
                                    .gpu_temp_c
                                    .map(|t| format!("{t} °C"))
                                    .unwrap_or_else(|| "-- °C".into());
                                let power = tel
                                    .gpu_power_w
                                    .map(|w| format!("{w:.1} W"))
                                    .unwrap_or_else(|| "-- W".into());
                                let usage = tel
                                    .gpu_usage_pct
                                    .map(|u| format!("{u:.0}%"))
                                    .unwrap_or_else(|| "--%".into());
                                let clock = tel
                                    .gpu_clock_mhz
                                    .map(|c| format!("{c} MHz"))
                                    .unwrap_or_else(|| "-- MHz".into());

                                ui.label(RichText::new(temp).size(24.0).strong().color(TEXT_ICE));
                                ui.add_space(12.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{}: {power}  |  {}: {usage}",
                                            s.gpu_power, s.gpu_usage
                                        ))
                                        .size(13.0)
                                        .color(ACCENT_CYAN_SOFT),
                                    );
                                    let vram_str = match (tel.gpu_vram_mb, tel.gpu_vram_total_mb) {
                                        (Some(used), Some(total)) if total > 0 => {
                                            let pct = (used as f32 / total as f32) * 100.0;
                                            format!("{used} / {total} MB ({pct:.0}%)")
                                        }
                                        (Some(used), _) => format!("{used} MB"),
                                        _ => "-- MB".to_string(),
                                    };
                                    ui.label(
                                        RichText::new(format!("Clock: {clock}  |  VRAM: {vram_str}"))
                                            .size(11.5)
                                            .color(TEXT_MUTED),
                                    );
                                });
                            });
                        });
                    });

                    ui.add_space(12.0);

                    // Real-time Charts (Separated CPU & GPU)
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

                            let cpu_pts: PlotPoints = cpu_hist
                                .iter()
                                .enumerate()
                                .map(|(i, v)| [i as f64, *v])
                                .collect();
                            let cpu_t_pts: PlotPoints = cpu_t_hist
                                .iter()
                                .enumerate()
                                .map(|(i, v)| [i as f64, *v])
                                .collect();

                            Plot::new("cpu_monitor_plot")
                                .height(165.0)
                                .include_x(0.0)
                                .include_x((HISTORY_LEN - 1) as f64)
                                .include_y(0.0)
                                .include_y(100.0)
                                .show_axes([false, true])
                                .show_grid(true)
                                .allow_drag(false)
                                .allow_zoom(false)
                                .allow_scroll(false)
                                .legend(Legend::default())
                                .show(ui, |plot_ui| {
                                    plot_ui.line(
                                        Line::new(cpu_pts)
                                            .name("CPU %")
                                            .color(ACCENT_ICE)
                                            .fill(0.0_f32)
                                            .fill_alpha(0.16_f32)
                                            .width(2.2_f32),
                                    );
                                    plot_ui.line(
                                        Line::new(cpu_t_pts)
                                            .name("CPU °C")
                                            .color(ACCENT_INDIGO)
                                            .width(2.0_f32),
                                    );
                                });
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

                            let gpu_pts: PlotPoints = gpu_hist
                                .iter()
                                .enumerate()
                                .map(|(i, v)| [i as f64, *v])
                                .collect();
                            let power_pts: PlotPoints = power_hist
                                .iter()
                                .enumerate()
                                .map(|(i, v)| [i as f64, *v])
                                .collect();
                            let gpu_t_pts: PlotPoints = gpu_t_hist
                                .iter()
                                .enumerate()
                                .map(|(i, v)| [i as f64, *v])
                                .collect();

                            Plot::new("gpu_monitor_plot")
                                .height(165.0)
                                .include_x(0.0)
                                .include_x((HISTORY_LEN - 1) as f64)
                                .include_y(0.0)
                                .include_y(100.0)
                                .show_axes([false, true])
                                .show_grid(true)
                                .allow_drag(false)
                                .allow_zoom(false)
                                .allow_scroll(false)
                                .legend(Legend::default())
                                .show(ui, |plot_ui| {
                                    plot_ui.line(
                                        Line::new(gpu_pts)
                                            .name("GPU %")
                                            .color(ACCENT_ICE)
                                            .fill(0.0_f32)
                                            .fill_alpha(0.16_f32)
                                            .width(2.2_f32),
                                    );
                                    plot_ui.line(
                                        Line::new(power_pts)
                                            .name("GPU W")
                                            .color(Color32::from_rgb(251, 191, 36))
                                            .width(2.0_f32),
                                    );
                                    plot_ui.line(
                                        Line::new(gpu_t_pts)
                                            .name("GPU °C")
                                            .color(Color32::from_rgb(52, 211, 153))
                                            .width(1.9_f32),
                                    );
                                });
                        });
                    });

                    ui.add_space(10.0);

                    // Detected Fans & Speeds (Model-agnostic)
                    crystal_card(ui, false, |ui| {
                        ui.label(
                            RichText::new(s.fans_card_title)
                                .size(13.5)
                                .strong()
                                .color(ACCENT_ICE),
                        );
                        ui.add_space(6.0);

                        if tel.fans.is_empty() {
                            ui.label(
                                RichText::new(s.fans_none_detected)
                                    .size(11.5)
                                    .color(TEXT_MUTED),
                            );
                        } else {
                            let max_temp = tel
                                .cpu_temp_c
                                .unwrap_or(0)
                                .max(tel.gpu_temp_c.unwrap_or(0));

                            ui.columns(tel.fans.len().min(4), |cols| {
                                for (idx, fan) in tel.fans.iter().enumerate() {
                                    let col = &mut cols[idx % cols.len()];
                                    let effective_pct = fan.pct.or(match self.mode {
                                        PowerMode::Turbo => Some(100),
                                        PowerMode::Manual(f1, f2) => {
                                            if idx == 0 {
                                                Some(raw_to_pct(f1))
                                            } else {
                                                Some(raw_to_pct(f2))
                                            }
                                        }
                                        PowerMode::Curve if max_temp > 0 => {
                                            Some(self.fan_curve.evaluate(max_temp))
                                        }
                                        _ => None,
                                    });

                                    let speed_text = match (fan.rpm, effective_pct) {
                                        (Some(rpm), Some(p)) => format!("{rpm} RPM ({p}%)"),
                                        (Some(rpm), None) => format!("{rpm} RPM"),
                                        (None, Some(p)) => format!("{p}%"),
                                        (None, None) => s.fan_auto_label.to_string(),
                                    };

                                    col.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(&fan.name)
                                                .size(12.0)
                                                .strong()
                                                .color(TEXT_ICE),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    RichText::new(&speed_text)
                                                        .size(12.0)
                                                        .strong()
                                                        .color(ACCENT_CYAN_SOFT),
                                                );
                                            },
                                        );
                                    });
                                    let bar_val = effective_pct.unwrap_or(0) as f32 / 100.0;
                                    col.add(
                                        egui::ProgressBar::new(bar_val)
                                            .desired_height(6.0)
                                            .fill(if matches!(self.mode, PowerMode::Turbo) {
                                                ACCENT_TURBO_GLOW
                                            } else {
                                                ACCENT_ICE
                                            }),
                                    );
                                }
                            });
                        }
                    });

                    ui.add_space(10.0);

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
                            ui.label(RichText::new(format!("({raw} / 2560)")).size(11.5).color(TEXT_MUTED));
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
                            ui.label(RichText::new(format!("({raw} / 2560)")).size(11.5).color(TEXT_MUTED));
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
    const SIZE: usize = 64;
    let mut rgba = Vec::with_capacity(SIZE * SIZE * 4);
    let center = (SIZE as f32 - 1.0) / 2.0;
    let radius = 29.5_f32;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let r = (dx * dx + dy * dy).sqrt();
            if r <= radius {
                let alpha = if r > radius - 1.0 {
                    ((radius - r).clamp(0.0, 1.0) * 255.0) as u8
                } else {
                    255
                };
                let in_left = (18..=24).contains(&x) && (16..=48).contains(&y);
                let in_right = (40..=46).contains(&x) && (16..=48).contains(&y);
                let in_diag = (20..=44).contains(&x) && (16..=48).contains(&y) && {
                    let ey = 16.0 + (x - 20) as f32 * (32.0 / 24.0);
                    (y as f32 - ey).abs() <= 3.2
                };
                if in_left || in_right || in_diag {
                    rgba.extend_from_slice(&[90, 210, 255, alpha]);
                } else if r >= 26.0 {
                    rgba.extend_from_slice(&[56, 189, 248, alpha]);
                } else {
                    rgba.extend_from_slice(&[13, 24, 46, alpha]);
                }
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    egui::IconData {
        rgba,
        width: SIZE as u32,
        height: SIZE as u32,
    }
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
