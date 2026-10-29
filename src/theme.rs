//! Tema visual: paleta, tipografía (Inter + iconos Phosphor) y componentes reutilizables.
//!
//! Los componentes animan sus estados (hover, pulsación, selección) con curvas suaves; las
//! animaciones duran lo justo para dar respuesta sin frenar el uso.

use std::hash::Hash;

use egui::emath::easing;
use egui::epaint::RectShape;
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontData, FontDefinitions, FontFamily, FontId, Id, Margin,
    Rect, Response, RichText, Sense, Shadow, Shape, Stroke, StrokeKind, TextStyle, Theme, Ui, pos2, vec2,
};

pub use egui_phosphor::regular as icon;
pub use rdmnsync_core::format::hours;

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
    sp.button_padding = vec2(14.0, 6.0);
    sp.interact_size.y = 30.0;
    sp.window_margin = Margin::same(18);
    sp.menu_margin = Margin::same(8);
    sp.combo_width = 220.0;
    sp.text_edit_width = 260.0;
    s.animation_time = 0.18;

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
    v.selection.stroke = Stroke::new(1.5, p.accent);
    v.striped = false;
    v.override_text_color = Some(p.text);
    v.interact_cursor = Some(CursorIcon::PointingHand);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;

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
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.7));
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 1.0;

    w.active.bg_fill = p.accent.gamma_multiply(0.5);
    w.active.weak_bg_fill = p.accent.gamma_multiply(0.30);
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open = w.hovered;
}

// ---- Animación --------------------------------------------------------------

/// Duración de las transiciones de hover.
const HOVER_SECS: f32 = 0.14;
/// Duración de los cambios de selección y de los indicadores que se desplazan.
const SLIDE_SECS: f32 = 0.24;

/// Intensidad animada (0→1) del hover de un widget habilitado.
pub fn hover_t(ui: &Ui, resp: &Response) -> f32 {
    ui.ctx().animate_bool_with_time_and_easing(
        resp.id.with("hover"),
        resp.hovered() && resp.enabled(),
        HOVER_SECS,
        easing::cubic_out,
    )
}

fn toggle_t(ui: &Ui, id: Id, on: bool) -> f32 {
    ui.ctx().animate_bool_with_time_and_easing(id.with("on"), on, SLIDE_SECS * 0.8, easing::cubic_out)
}

#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    start: f64,
}

/// Lleva un valor hacia `target` con una curva suave; si el objetivo cambia a mitad, sigue desde
/// donde estaba. La primera vez devuelve el objetivo tal cual.
pub fn ease_to(ui: &Ui, id: Id, target: f32, secs: f32) -> f32 {
    ease_to_from(ui, id, target, secs, target)
}

/// Como [`ease_to`], pero la primera vez arranca desde `initial` (p. ej. barras que crecen desde 0).
pub fn ease_to_from(ui: &Ui, id: Id, target: f32, secs: f32, initial: f32) -> f32 {
    let ctx = ui.ctx();
    let now = ctx.input(|i| i.time);
    let progress = |tw: &Tween| ((now - tw.start) as f32 / secs).clamp(0.0, 1.0);
    let value = |tw: &Tween| egui::lerp(tw.from..=tw.to, easing::cubic_out(progress(tw)));
    let mut tw = ctx.data(|d| d.get_temp::<Tween>(id)).unwrap_or(Tween { from: initial, to: target, start: now });
    if tw.to != target {
        tw = Tween { from: value(&tw), to: target, start: now };
    }
    ctx.data_mut(|d| d.insert_temp(id, tw));
    if progress(&tw) < 1.0 {
        ctx.request_repaint();
    }
    value(&tw)
}

fn ease_rect(ui: &Ui, id: Id, target: Rect, secs: f32) -> Rect {
    Rect::from_min_max(
        pos2(ease_to(ui, id.with(0), target.min.x, secs), ease_to(ui, id.with(1), target.min.y, secs)),
        pos2(ease_to(ui, id.with(2), target.max.x, secs), ease_to(ui, id.with(3), target.max.y, secs)),
    )
}

/// Progreso de aparición (0→1). Vuelve a empezar cada vez que el elemento reaparece tras no
/// haberse mostrado en la pasada anterior (cambio de pestaña, panel que se abre…).
pub fn appear(ui: &Ui, id: Id, secs: f32) -> f32 {
    let ctx = ui.ctx();
    let pass = ctx.cumulative_pass_nr();
    let now = ctx.input(|i| i.time);
    let (last, start) = ctx.data(|d| d.get_temp::<(u64, f64)>(id)).unwrap_or((u64::MAX, now));
    let start = if last == pass || last.wrapping_add(1) == pass { start } else { now };
    ctx.data_mut(|d| d.insert_temp(id, (pass, start)));
    let t = ((now - start) as f32 / secs).clamp(0.0, 1.0);
    if t < 1.0 {
        ctx.request_repaint();
    }
    easing::cubic_out(t)
}

/// Añade un botón sin fondo propio y pinta detrás el que devuelva `paint` según el hover animado.
fn button_with_bg(
    ui: &mut Ui,
    button: egui::Button<'_>,
    paint: impl FnOnce(&Response, f32) -> Vec<Shape>,
) -> Response {
    let slot = ui.painter().add(Shape::Noop);
    let resp = ui.add(button.fill(Color32::TRANSPARENT).stroke(Stroke::NONE));
    let t = hover_t(ui, &resp);
    ui.painter().set(slot, Shape::Vec(paint(&resp, t)));
    resp
}

// ---- Componentes ------------------------------------------------------------

pub fn card_frame(ui: &Ui) -> egui::Frame {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(12)
        .inner_margin(Margin::same(18))
        .shadow(Shadow {
            offset: [0, 2],
            blur: 10,
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
        egui::Frame::new()
            .fill(p.accent.gamma_multiply(0.12))
            .corner_radius(7)
            .inner_margin(Margin::same(4))
            .show(ui, |ui| {
                ui.label(RichText::new(ic).size(15.0).color(p.accent));
            });
        ui.label(RichText::new(title).size(16.0).family(semibold()));
    });
    ui.add_space(2.0);
}

pub fn page_header(ui: &mut Ui, title: &str, subtitle: &str) {
    let p = palette(ui);
    ui.add_space(4.0);
    ui.label(RichText::new(title).size(24.0).family(semibold()));
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(p.text_weak));
    }
    ui.add_space(14.0);
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

/// Botón de la acción principal: se ilumina y proyecta un halo del color de acento al pasar por
/// encima, y se oscurece al pulsarlo.
pub fn primary_button(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = palette(ui);
    let enabled = ui.is_enabled();
    let color = if enabled { p.on_accent } else { p.text_weak };
    let button = egui::Button::new(RichText::new(text.into()).color(color).family(semibold()))
        .corner_radius(8)
        .min_size(vec2(0.0, 34.0));
    button_with_bg(ui, button, |resp, t| {
        if !enabled {
            return vec![RectShape::filled(resp.rect, 8, p.control).into()];
        }
        let fill = if resp.is_pointer_button_down_on() {
            p.accent.lerp_to_gamma(Color32::BLACK, 0.14)
        } else {
            p.accent.lerp_to_gamma(Color32::WHITE, 0.12 * t)
        };
        let glow = Shadow {
            offset: [0, (2.0 + 3.0 * t) as i8],
            blur: (6.0 + 12.0 * t) as u8,
            spread: 0,
            color: p.accent.gamma_multiply(0.22 + 0.28 * t),
        };
        vec![glow.as_shape(resp.rect, 8).into(), RectShape::filled(resp.rect, 8, fill).into()]
    })
}

pub fn danger_button(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = palette(ui);
    let button = egui::Button::new(RichText::new(text.into()).color(p.danger)).corner_radius(8);
    button_with_bg(ui, button, |resp, t| {
        let fill = if resp.is_pointer_button_down_on() { 0.30 } else { 0.12 + 0.10 * t };
        vec![
            RectShape::new(
                resp.rect,
                8,
                p.danger.gamma_multiply(fill),
                Stroke::new(1.0, p.danger.gamma_multiply(0.4 + 0.4 * t)),
                StrokeKind::Inside,
            )
            .into(),
        ]
    })
}

fn icon_button_tinted(ui: &mut Ui, ic: &str, tooltip: &str, tint: Color32) -> Response {
    let p = palette(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let t = hover_t(ui, &resp);
    let bg = if resp.is_pointer_button_down_on() { 0.24 } else { 0.13 * t };
    ui.painter().rect_filled(rect, 8, tint.gamma_multiply(bg));
    let color = p.text_weak.lerp_to_gamma(tint, t);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, ic, FontId::proportional(16.0), color);
    resp.on_hover_text(tooltip)
}

/// Botón de solo icono con un fondo suave que aparece al pasar por encima.
pub fn icon_button(ui: &mut Ui, ic: &str, tooltip: &str) -> Response {
    let p = palette(ui);
    icon_button_tinted(ui, ic, tooltip, p.accent)
}

/// Como [`icon_button`], para acciones destructivas: se tiñe de rojo al pasar por encima.
pub fn icon_button_danger(ui: &mut Ui, ic: &str, tooltip: &str) -> Response {
    let p = palette(ui);
    icon_button_tinted(ui, ic, tooltip, p.danger)
}

/// Tarjeta de cifra destacada. El número cuenta hasta su valor cuando cambia.
pub fn stat(ui: &mut Ui, ic: &str, label: &str, value: usize, color: Color32) {
    let p = palette(ui);
    let shown = ease_to_from(ui, ui.scope_id().with(("stat", ic)), value as f32, 0.6, 0.0).round();
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
                ui.add_space(2.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new(format!("{shown:.0}")).size(20.0).family(semibold()));
                    ui.label(RichText::new(label).size(12.0).color(p.text_weak));
                });
            });
        });
}

pub fn empty_state(ui: &mut Ui, ic: &str, title: &str, text: &str) {
    let p = palette(ui);
    let t = appear(ui, ui.scope_id().with(("empty", title)), 0.35);
    ui.vertical_centered(|ui| {
        ui.multiply_opacity(t);
        ui.add_space(40.0 + 10.0 * (1.0 - t));
        let (rect, _) = ui.allocate_exact_size(vec2(84.0, 84.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 40.0, p.accent.gamma_multiply(0.08));
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            ic,
            FontId::proportional(42.0),
            p.accent.gamma_multiply(0.75),
        );
        ui.add_space(8.0);
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
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(ic).color(color).size(16.0));
                ui.label(text.into());
            });
        });
}

/// Indicador de carga centrado con un texto explicativo.
pub fn loading(ui: &mut Ui, text: &str) {
    let p = palette(ui);
    ui.vertical_centered(|ui| {
        ui.add_space(30.0);
        ui.add(egui::Spinner::new().size(26.0).color(p.accent));
        ui.add_space(4.0);
        ui.label(RichText::new(text).color(p.text_weak));
        ui.add_space(30.0);
    });
}

/// Filas de marcador con un brillo que las recorre, mientras llegan los datos.
pub fn skeleton(ui: &mut Ui, rows: usize) {
    let p = palette(ui);
    let time = ui.input(|i| i.time) as f32;
    let width = ui.available_width();
    for i in 0..rows {
        let (rect, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
        let pulse = 0.5 + 0.5 * (time * 3.0 - i as f32 * 0.45).sin();
        let color = p.control.lerp_to_gamma(p.control_hover, pulse);
        let text_w = (width - 200.0).max(40.0) * (0.45 + 0.4 * ((i * 37 % 10) as f32 / 10.0));
        let block = |x: f32, w: f32| Rect::from_min_size(pos2(rect.left() + x, rect.top() + 5.0), vec2(w, 12.0));
        ui.painter().rect_filled(block(8.0, 44.0), 6, color);
        ui.painter().rect_filled(block(68.0, text_w), 6, color);
        ui.painter().rect_filled(block((width - 96.0).max(68.0 + text_w + 12.0), 80.0), 6, color);
    }
    ui.ctx().request_repaint();
}

/// Fila seleccionable de lista, alineada a la izquierda y ocupando todo el ancho. Al seleccionarla
/// aparece una barra de acento a la izquierda; al pasar por encima el texto se desplaza un poco.
pub fn list_item(ui: &mut Ui, selected: bool, text: &str, indent: f32) -> Response {
    let p = palette(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    let t = hover_t(ui, &resp);
    let s = toggle_t(ui, resp.id, selected);
    let painter = ui.painter();
    painter.rect_filled(rect, 8, p.control.gamma_multiply(t * (1.0 - s)));
    painter.rect_filled(rect, 8, p.accent.gamma_multiply(0.16 * s));
    if s > 0.0 {
        let bar = Rect::from_center_size(pos2(rect.left() + 3.0, rect.center().y), vec2(3.0, 16.0 * s));
        painter.rect_filled(bar, 2, p.accent);
    }
    let color = p.text.lerp_to_gamma(p.accent, s);
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(14.0), Color32::PLACEHOLDER);
    let pos = rect.left_center() + vec2(10.0 + indent + 3.0 * t.max(s), -galley.size().y / 2.0);
    painter.with_clip_rect(rect.shrink(4.0)).galley(pos, galley, color);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Pastilla de texto clicable. Con `toggle` lleva fondo y borde propios que se tiñen al activarse;
/// sin él es transparente y el fondo de la seleccionada lo pinta [`segmented`].
fn chip(ui: &mut Ui, active: bool, text: &str, toggle: bool) -> Response {
    let p = palette(ui);
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(14.0), Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 26.0, 32.0), Sense::click());
    let t = hover_t(ui, &resp);
    let s = toggle_t(ui, resp.id, active);
    let painter = ui.painter();
    if toggle {
        let fill = p.control.lerp_to_gamma(p.control_hover, t).lerp_to_gamma(p.accent.gamma_multiply(0.22), s);
        let stroke = p.border.lerp_to_gamma(p.accent.gamma_multiply(0.7), t.max(s));
        painter.rect(rect, 8, fill, Stroke::new(1.0, stroke), StrokeKind::Inside);
    } else {
        painter.rect_filled(rect, 8, p.control.gamma_multiply(t * (1.0 - s)));
    }
    let rest = if toggle { p.text } else { p.text_weak };
    let color = rest.lerp_to_gamma(p.text, t).lerp_to_gamma(p.accent, s);
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    if resp.is_pointer_button_down_on() {
        painter.rect_filled(rect, 8, p.accent.gamma_multiply(0.10));
    }
    resp
}

/// Grupo de pestañas tipo «píldora». Un indicador se desliza hasta la seleccionada.
/// Devuelve el índice pulsado, si lo hay.
pub fn segmented(
    ui: &mut Ui,
    id_salt: impl Hash + std::fmt::Debug,
    selected: Option<usize>,
    labels: &[impl AsRef<str>],
) -> Option<usize> {
    let p = palette(ui);
    let id = ui.make_persistent_id(("segmented", id_salt));
    let slot = ui.painter().add(Shape::Noop);
    let mut clicked = None;
    let mut origin = None;
    let mut target = None;
    for (i, label) in labels.iter().enumerate() {
        let resp = chip(ui, selected == Some(i), label.as_ref(), false);
        let o = *origin.get_or_insert(resp.rect.min.to_vec2());
        if selected == Some(i) {
            // Relativo al grupo, para que el indicador no se retrase al desplazar la página.
            target = Some(resp.rect.translate(-o));
        }
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    match (target, origin) {
        (Some(target), Some(o)) => {
            let r = ease_rect(ui, id, target, SLIDE_SECS).translate(o);
            ui.painter().set(
                slot,
                RectShape::new(
                    r,
                    8,
                    p.accent.gamma_multiply(0.16),
                    Stroke::new(1.0, p.accent.gamma_multiply(0.35)),
                    StrokeKind::Inside,
                ),
            );
        }
        // Sin selección, olvida la posición: al volver a seleccionar aparece sin desplazarse.
        _ => ui.ctx().data_mut(|d| (0..4).for_each(|k| d.remove::<Tween>(id.with(k)))),
    }
    clicked
}

/// Filtro activable con aspecto de pastilla.
pub fn toggle_chip(ui: &mut Ui, on: &mut bool, text: impl Into<String>) -> Response {
    let mut resp = chip(ui, *on, &text.into(), true);
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    resp
}

/// Casilla de verificación: se rellena de acento y dibuja la marca con una transición.
/// Con `text` vacío se muestra solo la casilla.
pub fn checkbox(ui: &mut Ui, on: &mut bool, text: impl Into<String>) -> Response {
    let p = palette(ui);
    let text = text.into();
    let galley = (!text.is_empty())
        .then(|| ui.painter().layout_no_wrap(text, FontId::proportional(14.0), Color32::PLACEHOLDER));
    let side = 18.0;
    let label_w = galley.as_ref().map_or(0.0, |g| 8.0 + g.size().x);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(side + label_w, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = hover_t(ui, &resp);
    let s = toggle_t(ui, resp.id, *on);
    let painter = ui.painter();
    let bx = Rect::from_min_size(pos2(rect.left(), rect.center().y - side / 2.0), vec2(side, side));
    let fill = p.input.lerp_to_gamma(p.accent, s);
    let stroke = p.border.lerp_to_gamma(p.accent, t.max(s));
    painter.rect(bx.expand(t), 5, fill, Stroke::new(1.0, stroke), StrokeKind::Inside);
    if s > 0.0 {
        let c = bx.shrink(4.5);
        let check = [pos2(c.left(), c.center().y), pos2(c.left() + c.width() * 0.38, c.bottom()), pos2(c.right(), c.top())];
        painter.line(check.to_vec(), Stroke::new(2.0, p.on_accent.gamma_multiply(s)));
    }
    if let Some(g) = galley {
        painter.galley(pos2(bx.right() + 8.0, rect.center().y - g.size().y / 2.0), g, p.text);
    }
    resp
}

/// Interruptor con etiqueta; toda la fila es clicable y la bola se desliza al cambiar.
pub fn switch(ui: &mut Ui, on: &mut bool, text: impl Into<String>) -> Response {
    let p = palette(ui);
    let track = vec2(36.0, 20.0);
    let wrap = (ui.available_width() - track.x - 10.0).max(80.0);
    let galley = ui.painter().layout(text.into(), FontId::proportional(14.0), Color32::PLACEHOLDER, wrap);
    let size = vec2(track.x + 10.0 + galley.size().x, galley.size().y.max(track.y));
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = hover_t(ui, &resp);
    let s = toggle_t(ui, resp.id, *on);
    let painter = ui.painter();
    let track_rect = Rect::from_min_size(pos2(rect.left(), rect.top() + (size.y.min(24.0) - track.y) / 2.0), track);
    let fill = p.control.lerp_to_gamma(p.accent, s);
    let stroke = p.border.lerp_to_gamma(p.accent, t.max(s) * 0.8);
    painter.rect(track_rect, 10, fill, Stroke::new(1.0, stroke), StrokeKind::Inside);
    let x = egui::lerp(track_rect.left() + 10.0..=track_rect.right() - 10.0, s);
    let knob = pos2(x, track_rect.center().y);
    painter.circle_filled(knob + vec2(0.0, 1.0), 7.5, Color32::from_black_alpha(40));
    painter.circle_filled(knob, 7.0 + 0.6 * t, Color32::WHITE);
    let text_pos = pos2(track_rect.right() + 10.0, rect.top() + (size.y - galley.size().y) / 2.0);
    painter.galley(text_pos, galley, p.text);
    resp
}

/// Logotipo: cuadrado redondeado con las flechas de sincronización.
pub fn logo(ui: &mut Ui, size: f32) {
    let p = palette(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let glow = Shadow {
        offset: [0, (size * 0.08) as i8],
        blur: (size * 0.4) as u8,
        spread: 0,
        color: p.accent.gamma_multiply(0.35),
    };
    ui.painter().add(glow.as_shape(rect, size * 0.28));
    ui.painter().rect_filled(rect, size * 0.28, p.accent);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon::ARROWS_LEFT_RIGHT,
        FontId::proportional(size * 0.58),
        p.on_accent,
    );
}

/// Punto de estado. Con `pulse`, emite una onda para indicar que algo está en curso.
pub fn status_dot(ui: &mut Ui, color: Color32, pulse: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    if pulse {
        let t = (ui.input(|i| i.time) as f32 % 1.2) / 1.2;
        ui.painter().circle_filled(rect.center(), 4.0 + 5.0 * t, color.gamma_multiply(0.45 * (1.0 - t)));
        ui.ctx().request_repaint();
    }
    ui.painter().circle_filled(rect.center(), 4.0, color);
}
