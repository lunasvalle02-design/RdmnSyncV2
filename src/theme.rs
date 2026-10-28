//! Tema visual: paleta, tipografía (Inter + iconos Phosphor) y componentes reutilizables.

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Response,
    RichText, Shadow, Stroke, TextStyle, Theme, Ui, Vec2, vec2,
};

pub use egui_phosphor::regular as icon;

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub card: Color32,
    pub control: Color32,
    pub control_hover: Color32,
    pub input: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub info: Color32,
}

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x0E, 0x10, 0x17),
    surface: Color32::from_rgb(0x13, 0x16, 0x20),
    card: Color32::from_rgb(0x19, 0x1D, 0x2A),
    control: Color32::from_rgb(0x23, 0x28, 0x38),
    control_hover: Color32::from_rgb(0x2C, 0x32, 0x46),
    input: Color32::from_rgb(0x10, 0x13, 0x1C),
    border: Color32::from_rgb(0x2A, 0x30, 0x44),
    text: Color32::from_rgb(0xE7, 0xE9, 0xF1),
    text_weak: Color32::from_rgb(0x8C, 0x93, 0xAB),
    accent: Color32::from_rgb(0x7C, 0x7F, 0xFF),
    on_accent: Color32::WHITE,
    success: Color32::from_rgb(0x3D, 0xD6, 0x8C),
    warning: Color32::from_rgb(0xF5, 0xB9, 0x4A),
    danger: Color32::from_rgb(0xF2, 0x6D, 0x6D),
    info: Color32::from_rgb(0x52, 0xB6, 0xFF),
};

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xF3, 0xF4, 0xF8),
    surface: Color32::from_rgb(0xFA, 0xFA, 0xFC),
    card: Color32::WHITE,
    control: Color32::from_rgb(0xEE, 0xEF, 0xF5),
    control_hover: Color32::from_rgb(0xE3, 0xE5, 0xEF),
    input: Color32::from_rgb(0xF7, 0xF8, 0xFB),
    border: Color32::from_rgb(0xE0, 0xE3, 0xEC),
    text: Color32::from_rgb(0x1A, 0x1D, 0x2B),
    text_weak: Color32::from_rgb(0x67, 0x6E, 0x86),
    accent: Color32::from_rgb(0x58, 0x5C, 0xF0),
    on_accent: Color32::WHITE,
    success: Color32::from_rgb(0x16, 0xA3, 0x5F),
    warning: Color32::from_rgb(0xC9, 0x85, 0x0A),
    danger: Color32::from_rgb(0xD9, 0x3F, 0x3F),
    info: Color32::from_rgb(0x1E, 0x88, 0xE5),
};

pub fn palette(ui: &Ui) -> Palette {
    if ui.visuals().dark_mode { DARK } else { LIGHT }
}

pub const SEMIBOLD: &str = "semibold";

pub fn semibold() -> FontFamily {
    FontFamily::Name(SEMIBOLD.into())
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "inter-semibold".into(),
        FontData::from_static(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")).into(),
    );
    fonts
        .font_data
        .insert("phosphor".into(), FontData::from_static(egui_phosphor::Variant::Regular.font_bytes()).into());

    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    prop.insert(0, "inter".into());
    prop.insert(1, "phosphor".into());
    let mut semi = vec!["inter-semibold".to_string(), "phosphor".to_string()];
    semi.extend(fallback);
    fonts.families.insert(semibold(), semi);
    if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
        mono.push("phosphor".into());
    }
    ctx.set_fonts(fonts);

    ctx.style_mut_of(Theme::Dark, |s| style(s, &DARK, true));
    ctx.style_mut_of(Theme::Light, |s| style(s, &LIGHT, false));
}

fn style(s: &mut egui::Style, p: &Palette, dark: bool) {
    s.text_styles = [
        (TextStyle::Heading, FontId::new(22.0, semibold())),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();

    let sp = &mut s.spacing;
    sp.item_spacing = vec2(8.0, 8.0);
    sp.button_padding = vec2(12.0, 6.0);
    sp.interact_size.y = 30.0;
    sp.window_margin = Margin::same(18);
    sp.menu_margin = Margin::same(8);
    sp.combo_width = 220.0;
    sp.text_edit_width = 260.0;

    let v = &mut s.visuals;
    *v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.extreme_bg_color = p.input;
    v.text_edit_bg_color = Some(p.input);
    v.faint_bg_color = if dark {
        Color32::from_rgba_unmultiplied(255, 255, 255, 6)
    } else {
        Color32::from_rgba_unmultiplied(0, 0, 40, 8)
    };
    v.code_bg_color = p.control;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_shadow = Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 140 } else { 40 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 110 } else { 30 }),
    };
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.striped = false;
    v.override_text_color = Some(p.text);

    let r = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.card;
    w.noninteractive.weak_bg_fill = p.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = p.control;
    w.inactive.weak_bg_fill = p.control;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = p.control_hover;
    w.hovered.weak_bg_fill = p.control_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.6));
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.accent.gamma_multiply(0.5);
    w.active.weak_bg_fill = p.accent.gamma_multiply(0.35);
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open = w.hovered;
}

// ---- Componentes ------------------------------------------------------------

pub fn card_frame(ui: &Ui) -> egui::Frame {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(12)
        .inner_margin(Margin::same(16))
        .shadow(Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: Color32::from_black_alpha(if ui.visuals().dark_mode { 50 } else { 12 }),
        })
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame(ui)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn section_title(ui: &mut Ui, ic: &str, title: &str) {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.label(RichText::new(ic).size(17.0).color(p.accent));
        ui.label(RichText::new(title).size(16.0).family(semibold()));
    });
}

pub fn page_header(ui: &mut Ui, title: &str, subtitle: &str) {
    let p = palette(ui);
    ui.add_space(4.0);
    ui.label(RichText::new(title).size(24.0).family(semibold()));
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(p.text_weak));
    }
    ui.add_space(8.0);
}

pub fn badge(ui: &mut Ui, text: impl Into<String>, color: Color32) -> Response {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.16))
        .corner_radius(CornerRadius::same(99))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text.into()).size(12.0).color(color));
        })
        .response
}

pub fn confidence_badge(ui: &mut Ui, score: f32) -> Response {
    let p = palette(ui);
    let color = if score >= 0.9 {
        p.success
    } else if score >= 0.6 {
        p.warning
    } else {
        p.danger
    };
    badge(ui, format!("{:.0}%", score * 100.0), color)
}

pub fn primary_button(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = palette(ui);
    let enabled = ui.is_enabled();
    let fill = if enabled { p.accent } else { p.control };
    let color = if enabled { p.on_accent } else { p.text_weak };
    let resp = ui.add(
        egui::Button::new(RichText::new(text.into()).color(color).family(semibold()))
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(8)
            .min_size(vec2(0.0, 34.0)),
    );
    if resp.hovered() && enabled {
        ui.painter().rect_filled(resp.rect, 8, Color32::from_white_alpha(14));
    }
    resp
}

pub fn danger_button(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = palette(ui);
    ui.add(
        egui::Button::new(RichText::new(text.into()).color(p.danger))
            .fill(p.danger.gamma_multiply(0.12))
            .stroke(Stroke::new(1.0, p.danger.gamma_multiply(0.4)))
            .corner_radius(8),
    )
}

pub fn icon_button(ui: &mut Ui, ic: &str, tooltip: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(ic).size(16.0)).frame(false).min_size(vec2(26.0, 26.0)))
        .on_hover_text(tooltip)
}

pub fn stat(ui: &mut Ui, ic: &str, label: &str, value: String, color: Color32) {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(12)
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_min_width(190.0);
            ui.set_height(44.0);
            ui.horizontal_centered(|ui| {
                egui::Frame::new()
                    .fill(color.gamma_multiply(0.15))
                    .corner_radius(10)
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(RichText::new(ic).size(20.0).color(color));
                    });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new(value).size(20.0).family(semibold()));
                    ui.label(RichText::new(label).size(12.0).color(p.text_weak));
                });
            });
        });
}

pub fn empty_state(ui: &mut Ui, ic: &str, title: &str, text: &str) {
    let p = palette(ui);
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.label(RichText::new(ic).size(46.0).color(p.text_weak.gamma_multiply(0.6)));
        ui.add_space(6.0);
        ui.label(RichText::new(title).size(17.0).family(semibold()));
        ui.label(RichText::new(text).color(p.text_weak));
        ui.add_space(40.0);
    });
}

pub fn callout(ui: &mut Ui, ic: &str, color: Color32, text: impl Into<String>) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.10))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
        .corner_radius(10)
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(ic).color(color).size(16.0));
                ui.label(text.into());
            });
        });
}

/// Fila seleccionable de lista, alineada a la izquierda y ocupando todo el ancho.
pub fn list_item(ui: &mut Ui, selected: bool, text: &str, indent: f32) -> Response {
    let p = palette(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), egui::Sense::click());
    let fill = if selected {
        p.accent.gamma_multiply(0.16)
    } else if resp.hovered() {
        p.control
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 8, fill);
    let color = if selected { p.accent } else { p.text };
    let font = FontId::proportional(14.0);
    let galley = ui.painter().layout(text.to_string(), font, color, f32::INFINITY);
    let pos = rect.left_center() + vec2(10.0 + indent, -galley.size().y / 2.0);
    ui.painter().with_clip_rect(rect.shrink(4.0)).galley(pos, galley, color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Pestaña/segmento seleccionable con estilo de "píldora".
pub fn pill(ui: &mut Ui, selected: bool, text: impl Into<String>) -> Response {
    let p = palette(ui);
    let (fill, color) = if selected {
        (p.accent.gamma_multiply(0.18), p.accent)
    } else {
        (Color32::TRANSPARENT, p.text_weak)
    };
    ui.add(
        egui::Button::new(RichText::new(text.into()).color(color))
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(8)
            .min_size(Vec2::new(0.0, 32.0)),
    )
}

/// Logotipo: cuadrado redondeado con las flechas de sincronización.
pub fn logo(ui: &mut Ui, size: f32) {
    let p = palette(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), egui::Sense::hover());
    ui.painter().rect_filled(rect, size * 0.28, p.accent);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon::ARROWS_LEFT_RIGHT,
        FontId::proportional(size * 0.58),
        p.on_accent,
    );
}

pub fn status_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

pub fn hours(h: f64) -> String {
    let s = format!("{h:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s} h")
}
