//! Ajustes de la aplicación.

use egui::{RichText, Theme, Ui, vec2};

use crate::app::{App, weak};
use crate::store::Store;
use crate::theme::{self, icon};

pub fn show(app: &mut App, ui: &mut Ui) {
    let mut changed = false;
    let mut manage = false;
    crate::app::page(ui, "settings_page", |ui| {
        theme::page_header(ui, "Ajustes", "");
        let s = &mut app.store.settings;

        theme::section_title(ui, icon::PALETTE, "Apariencia");
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                if theme::pill(ui, s.dark_mode, format!("{}  Oscuro", icon::MOON)).clicked() && !s.dark_mode {
                    s.dark_mode = true;
                    ui.ctx().set_theme(Theme::Dark);
                    changed = true;
                }
                if theme::pill(ui, !s.dark_mode, format!("{}  Claro", icon::SUN)).clicked() && s.dark_mode {
                    s.dark_mode = false;
                    ui.ctx().set_theme(Theme::Light);
                    changed = true;
                }
            });
        });
        ui.add_space(12.0);

        theme::section_title(ui, icon::ARROWS_CLOCKWISE, "Sincronización");
        theme::card(ui, |ui| {
            changed |= ui
                .checkbox(&mut s.add_marker, "Añadir la marca [sync:origen#id] al comentario en el Redmine de destino")
                .changed();
            ui.label(weak(
                ui,
                "Permite detectar imputaciones ya sincronizadas aunque se pierda el historial local. \
                 El Redmine de origen nunca se modifica.",
            ));
            ui.add_space(10.0);
            egui::Grid::new("thresholds").num_columns(2).spacing(vec2(16.0, 10.0)).show(ui, |ui| {
                ui.label("Emparejar issues sin preguntar desde");
                changed |= ui
                    .add(egui::Slider::new(&mut s.auto_threshold, 0.75..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)))
                    .changed();
                ui.end_row();
                ui.label("Proponer candidatos desde");
                changed |= ui
                    .add(egui::Slider::new(&mut s.ask_threshold, 0.3..=0.9).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)))
                    .changed();
                ui.end_row();
            });
            if s.ask_threshold > s.auto_threshold {
                s.ask_threshold = s.auto_threshold;
            }
            ui.label(weak(
                ui,
                "Por debajo del segundo umbral se sugiere crear el issue en el destino. \
                 Una referencia explícita (la URL del issue de origen) siempre cuenta como 100 %.",
            ));
        });
        ui.add_space(12.0);

        theme::section_title(ui, icon::NOTE_PENCIL, "Actualización de issues de destino");
        theme::card(ui, |ui| {
            changed |= ui
                .checkbox(&mut s.update_issues, "Igualar los issues de destino con los de origen al sincronizar")
                .changed();
            ui.label(weak(ui, "Solo se modifica el issue de destino; el de origen se usa únicamente como fuente de datos."));
            ui.add_space(6.0);
            ui.add_enabled_ui(s.update_issues, |ui| {
                let f = &mut s.issue_fields;
                ui.horizontal_wrapped(|ui| {
                    changed |= ui.checkbox(&mut f.status, "Estado").changed();
                    changed |= ui.checkbox(&mut f.done_ratio, "% realizado").changed();
                    changed |= ui.checkbox(&mut f.dates, "Fechas de inicio y fin").changed();
                    changed |= ui.checkbox(&mut f.estimated, "Estimación").changed();
                    changed |= ui.checkbox(&mut f.version, "Versión prevista").changed();
                    changed |= ui.checkbox(&mut f.priority, "Prioridad").changed();
                    changed |= ui.checkbox(&mut f.subject, "Asunto").changed();
                    changed |= ui.checkbox(&mut f.description, "Descripción").changed();
                });
                ui.add_space(6.0);
                changed |= ui
                    .checkbox(&mut s.respect_dst_changes, "Respetar los cambios hechos directamente en el destino")
                    .changed();
                ui.label(weak(
                    ui,
                    "Si un campo no ha cambiado en el origen desde la última sincronización, no se                      sobrescribe aunque alguien lo haya modificado en el destino.",
                ));
            });
        });
        ui.add_space(12.0);

        theme::section_title(ui, icon::DATABASE, "Datos");
        theme::card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(weak(ui, "Configuración y mapeos:"));
                ui.label(RichText::new(Store::path().display().to_string()).monospace());
            });
            ui.label(weak(ui, "Las API keys recordadas se guardan en el almacén de credenciales del sistema."));
            ui.add_space(6.0);
            if ui.button(format!("{}  Gestionar conexiones", icon::PLUGS)).clicked() {
                manage = true;
            }
        });
        ui.add_space(12.0);
        ui.label(weak(ui, format!("RdmnSync {}", env!("CARGO_PKG_VERSION"))));
    });
    if manage {
        app.show_login = true;
    }
    if changed {
        app.save();
    }
}
