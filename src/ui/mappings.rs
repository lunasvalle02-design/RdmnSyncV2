//! Gestión de los mapeos aprendidos (por pareja origen → destino) y del historial con deshacer.

use std::collections::BTreeMap;

use egui::{Align, Layout, Margin, RichText, TextEdit, Ui, vec2};
use egui_extras::{Column, TableBuilder};

use crate::app::{App, Msg, ToastKind, dates, weak};
use crate::matching::normalize;
use crate::redmine::Project;
use crate::store::{CommentRule, ProjectLink, pair_key};
use crate::theme::{self, icon};

#[derive(Default)]
pub struct MappingsState {
    src: Option<String>,
    dst: Option<String>,
    issue_filter: String,
    new_act_src: Option<String>,
    new_act_dst: Option<String>,
    new_rule_text: String,
    new_rule_act: Option<String>,
    new_proj_src: Option<(u32, String)>,
    new_proj_dst: Option<(u32, String)>,
    undoing: Option<String>,
}

pub fn on_undone(app: &mut App, batch: String, ok: Vec<u32>, errors: Vec<String>) {
    app.mappings.undoing = None;
    for l in app.store.ledger.iter_mut().filter(|l| l.batch == batch && ok.contains(&l.dst_entry_id)) {
        l.undone = true;
    }
    app.save();
    if errors.is_empty() {
        app.toast(ToastKind::Success, format!("Deshechas {} imputaciones", ok.len()));
    } else {
        app.toast(ToastKind::Warning, format!("Deshechas {}; errores: {}", ok.len(), errors.join("; ")));
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let ids: Vec<String> = app.store.instances.iter().map(|i| i.id.clone()).collect();
    if app.mappings.src.as_ref().is_none_or(|s| !ids.contains(s)) {
        app.mappings.src = app.connected_ids().first().cloned().or_else(|| ids.first().cloned());
    }
    if app.mappings.dst.as_ref().is_none_or(|d| !ids.contains(d) || Some(d) == app.mappings.src.as_ref()) {
        app.mappings.dst = ids.iter().find(|i| Some(*i) != app.mappings.src.as_ref()).cloned();
    }

    crate::app::page(ui, "mappings_page", |ui| {
        theme::page_header(
            ui,
            "Mapeos",
            "Lo que la aplicación ha aprendido para cada pareja de Redmines. Los mapeos son dirigidos (origen → destino).",
        );
        if ids.len() < 2 {
            theme::empty_state(ui, icon::GIT_DIFF, "Sin parejas", "Configura al menos dos Redmines.");
            return;
        }
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(weak(ui, "Origen"));
                combo(ui, "map_src", &mut app.mappings.src, &ids, &app.store);
                ui.label(RichText::new(icon::ARROW_RIGHT).color(theme::palette(ui).text_weak));
                ui.label(weak(ui, "Destino"));
                combo(ui, "map_dst", &mut app.mappings.dst, &ids, &app.store);
            });
        });
        let (Some(src), Some(dst)) = (app.mappings.src.clone(), app.mappings.dst.clone()) else { return };
        if src == dst {
            return;
        }
        ui.add_space(12.0);
        activities(app, ui, &src, &dst);
        ui.add_space(12.0);
        rules(app, ui, &src, &dst);
        ui.add_space(12.0);
        projects(app, ui, &src, &dst);
        ui.add_space(12.0);
        issues(app, ui, &src, &dst);
        ui.add_space(12.0);
        history(app, ui, &src, &dst);
    });
}

impl App {
    fn activity_names(&self, inst: &str) -> Vec<String> {
        self.sessions.get(inst).map(|s| s.activities.iter().map(|a| a.name.clone()).collect()).unwrap_or_default()
    }
}

fn combo(ui: &mut Ui, id: &str, value: &mut Option<String>, ids: &[String], store: &crate::store::Store) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.as_ref().map(|v| store.instance_name(v)).unwrap_or_default())
        .width(200.0)
        .show_ui(ui, |ui| {
            for i in ids {
                ui.selectable_value(value, Some(i.clone()), store.instance_name(i));
            }
        });
}

fn name_combo(ui: &mut Ui, id: impl egui::AsIdSalt, value: &mut Option<String>, options: &[String], width: f32) -> bool {
    let before = value.clone();
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.clone().unwrap_or_else(|| "Elegir…".into()))
        .width(width)
        .show_ui(ui, |ui| {
            for o in options {
                ui.selectable_value(value, Some(o.clone()), o);
            }
        });
    *value != before
}

fn activities(app: &mut App, ui: &mut Ui, src: &str, dst: &str) {
    let p = theme::palette(ui);
    let src_names = app.activity_names(src);
    let dst_names = app.activity_names(dst);
    let mut pair = app.store.pair(src, dst);
    let mut changed = false;
    theme::section_title(ui, icon::TAG, "Actividades");
    theme::card(ui, |ui| {
        if pair.activities.is_empty() {
            ui.label(weak(ui, "Aún no hay actividades mapeadas. Se irán guardando al sincronizar."));
        }
        let mut remove = None;
        egui::Grid::new("map_acts").num_columns(4).spacing(vec2(12.0, 8.0)).striped(false).show(ui, |ui| {
            for (key, link) in pair.activities.iter_mut() {
                ui.label(RichText::new(&link.src_name).family(theme::semibold()));
                ui.label(RichText::new(icon::ARROW_RIGHT).color(p.text_weak));
                if dst_names.is_empty() {
                    ui.label(&link.dst_name);
                } else {
                    let mut v = Some(link.dst_name.clone());
                    if name_combo(ui, ("map_act", key.as_str()), &mut v, &dst_names, 220.0) {
                        link.dst_name = v.unwrap_or_default();
                        changed = true;
                    }
                }
                if theme::icon_button(ui, icon::TRASH, "Olvidar este mapeo").clicked() {
                    remove = Some(key.clone());
                }
                ui.end_row();
            }
        });
        if let Some(k) = remove {
            pair.activities.remove(&k);
            changed = true;
        }
        if !src_names.is_empty() && !dst_names.is_empty() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(weak(ui, "Añadir"));
                let st = &mut app.mappings;
                name_combo(ui, "new_act_src", &mut st.new_act_src, &src_names, 200.0);
                ui.label(RichText::new(icon::ARROW_RIGHT).color(p.text_weak));
                name_combo(ui, "new_act_dst", &mut st.new_act_dst, &dst_names, 200.0);
                let ok = st.new_act_src.is_some() && st.new_act_dst.is_some();
                if ui.add_enabled(ok, egui::Button::new(format!("{}  Guardar", icon::PLUS))).clicked() {
                    pair.set_activity(st.new_act_src.as_deref().unwrap_or_default(), st.new_act_dst.as_deref().unwrap_or_default());
                    st.new_act_src = None;
                    st.new_act_dst = None;
                    changed = true;
                }
            });
        } else {
            ui.label(weak(ui, "Conecta ambos Redmines para editar con listas desplegables."));
        }
    });
    if changed {
        *app.store.pair_mut(src, dst) = pair;
        app.save();
    }
}

fn rules(app: &mut App, ui: &mut Ui, src: &str, dst: &str) {
    let dst_names = app.activity_names(dst);
    let mut pair = app.store.pair(src, dst);
    let mut changed = false;
    theme::section_title(ui, icon::MAGIC_WAND, "Reglas por comentario");
    theme::card(ui, |ui| {
        ui.label(weak(ui, "Fuerzan la actividad de destino cuando el comentario de la imputación contiene un texto (sin distinguir mayúsculas ni acentos). Tienen prioridad sobre el mapeo de actividades."));
        ui.add_space(4.0);
        let mut remove = None;
        for (i, rule) in pair.rules.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(weak(ui, "Si contiene"));
                changed |= ui.add(TextEdit::singleline(&mut rule.contains).desired_width(200.0)).changed();
                ui.label(weak(ui, "usar"));
                let mut v = Some(rule.dst_activity.clone());
                if dst_names.is_empty() {
                    changed |= ui.add(TextEdit::singleline(&mut rule.dst_activity).desired_width(180.0)).changed();
                } else if name_combo(ui, ("rule", i), &mut v, &dst_names, 200.0) {
                    rule.dst_activity = v.unwrap_or_default();
                    changed = true;
                }
                if theme::icon_button(ui, icon::TRASH, "Eliminar regla").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            pair.rules.remove(i);
            changed = true;
        }
        ui.horizontal(|ui| {
            let st = &mut app.mappings;
            ui.label(weak(ui, "Nueva:"));
            ui.add(TextEdit::singleline(&mut st.new_rule_text).hint_text("p. ej. reunión").desired_width(200.0));
            ui.label(weak(ui, "usar"));
            name_combo(ui, "new_rule_act", &mut st.new_rule_act, &dst_names, 200.0);
            let ok = !st.new_rule_text.trim().is_empty() && st.new_rule_act.is_some();
            if ui.add_enabled(ok, egui::Button::new(format!("{}  Añadir", icon::PLUS))).clicked() {
                pair.rules.push(CommentRule {
                    contains: st.new_rule_text.trim().to_string(),
                    dst_activity: st.new_rule_act.take().unwrap_or_default(),
                });
                st.new_rule_text.clear();
                changed = true;
            }
        });
    });
    if changed {
        *app.store.pair_mut(src, dst) = pair;
        app.save();
    }
}

fn project_combo(
    ui: &mut Ui,
    id: impl egui::AsIdSalt,
    value: &mut Option<(u32, String)>,
    options: &[Project],
    width: f32,
) -> bool {
    let before = value.clone();
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.as_ref().map(|v| v.1.clone()).unwrap_or_else(|| "Elegir…".into()))
        .width(width)
        .show_ui(ui, |ui| {
            for p in options {
                ui.selectable_value(value, Some((p.id, p.name.clone())), &p.name);
            }
        });
    *value != before
}

fn projects(app: &mut App, ui: &mut Ui, src: &str, dst: &str) {
    let p = theme::palette(ui);
    let src_projects = app.sessions.get(src).map(|s| s.projects.clone()).unwrap_or_default();
    let dst_projects = app.sessions.get(dst).map(|s| s.projects.clone()).unwrap_or_default();
    let mut pair = app.store.pair(src, dst);
    let mut changed = false;
    theme::section_title(ui, icon::FOLDERS, "Proyectos");
    theme::card(ui, |ui| {
        ui.label(weak(
            ui,
            "Se usan para las imputaciones sin issue, para buscar issues equivalentes y como proyecto \
             por defecto al crear un issue.",
        ));
        ui.add_space(4.0);
        if pair.projects.is_empty() {
            ui.label(weak(ui, "Sin proyectos mapeados."));
        }
        let mut remove = None;
        egui::Grid::new("map_projects").num_columns(4).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
            for (id, link) in pair.projects.iter_mut() {
                ui.label(RichText::new(&link.src_name).family(theme::semibold()));
                ui.label(RichText::new(icon::ARROW_RIGHT).color(p.text_weak));
                if dst_projects.is_empty() {
                    ui.label(&link.dst_name);
                } else {
                    let mut v = Some((link.dst_id, link.dst_name.clone()));
                    if project_combo(ui, ("map_proj", *id), &mut v, &dst_projects, 280.0)
                        && let Some((did, dname)) = v
                    {
                        link.dst_id = did;
                        link.dst_name = dname;
                        changed = true;
                    }
                }
                if theme::icon_button(ui, icon::TRASH, "Olvidar este mapeo").clicked() {
                    remove = Some(*id);
                }
                ui.end_row();
            }
        });
        if let Some(id) = remove {
            pair.projects.remove(&id);
            changed = true;
        }
        if src_projects.is_empty() || dst_projects.is_empty() {
            ui.label(weak(ui, "Conecta ambos Redmines para añadir o editar proyectos."));
            return;
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(weak(ui, "Añadir"));
            let st = &mut app.mappings;
            let unmapped: Vec<Project> =
                src_projects.iter().filter(|p| !pair.projects.contains_key(&p.id)).cloned().collect();
            project_combo(ui, "new_proj_src", &mut st.new_proj_src, &unmapped, 240.0);
            ui.label(RichText::new(icon::ARROW_RIGHT).color(p.text_weak));
            project_combo(ui, "new_proj_dst", &mut st.new_proj_dst, &dst_projects, 260.0);
            let ok = st.new_proj_src.is_some() && st.new_proj_dst.is_some();
            if ui.add_enabled(ok, egui::Button::new(format!("{}  Guardar", icon::PLUS))).clicked()
                && let (Some((sid, sname)), Some((did, dname))) = (st.new_proj_src.take(), st.new_proj_dst.take())
            {
                pair.projects.insert(sid, ProjectLink { src_name: sname, dst_id: did, dst_name: dname });
                changed = true;
            }
        });
    });
    if changed {
        *app.store.pair_mut(src, dst) = pair;
        app.save();
    }
}

fn issues(app: &mut App, ui: &mut Ui, src: &str, dst: &str) {
    let p = theme::palette(ui);
    let pair = app.store.pair(src, dst);
    let filter = normalize(&app.mappings.issue_filter);
    let list: Vec<_> = pair
        .issues
        .iter()
        .filter(|(id, l)| {
            filter.is_empty()
                || normalize(&l.src_subject).contains(&filter)
                || normalize(&l.dst_subject).contains(&filter)
                || id.to_string().contains(&filter)
                || l.dst_id.to_string().contains(&filter)
        })
        .collect();
    let mut remove = None;
    ui.horizontal(|ui| {
        theme::section_title(ui, icon::LINK, "Issues enlazados");
        theme::badge(ui, pair.issues.len().to_string(), p.accent);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(TextEdit::singleline(&mut app.mappings.issue_filter).hint_text(format!("{}  Filtrar", icon::FUNNEL)).desired_width(200.0));
        });
    });
    theme::card_frame(ui).inner_margin(Margin::same(8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if list.is_empty() {
            ui.label(weak(ui, "  Sin issues enlazados."));
            return;
        }
        TableBuilder::new(ui)
            .id_salt("map_issues")
            .striped(true)
            .vscroll(false)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::remainder().at_least(200.0).clip(true))
            .column(Column::exact(20.0))
            .column(Column::remainder().at_least(200.0).clip(true))
            .column(Column::exact(100.0))
            .column(Column::exact(32.0))
            .header(24.0, |mut h| {
                for t in ["Origen", "", "Destino", "Origen del enlace", ""] {
                    h.col(|ui| {
                        ui.label(weak(ui, t).size(12.0));
                    });
                }
            })
            .body(|mut body| {
                for (id, l) in &list {
                    body.row(30.0, |mut row| {
                        row.col(|ui| {
                            ui.add(egui::Label::new(format!("#{id} {}", l.src_subject)).truncate());
                        });
                        row.col(|ui| {
                            ui.label(weak(ui, icon::ARROW_RIGHT));
                        });
                        row.col(|ui| {
                            ui.add(egui::Label::new(format!("#{} {}", l.dst_id, l.dst_subject)).truncate());
                        });
                        row.col(|ui| {
                            let color = match l.origin {
                                crate::store::LinkOrigin::Created => p.success,
                                crate::store::LinkOrigin::Confirmed => p.info,
                                crate::store::LinkOrigin::Auto => p.warning,
                            };
                            theme::badge(ui, l.origin.label(), color);
                        });
                        row.col(|ui| {
                            if theme::icon_button(ui, icon::LINK_BREAK, "Desenlazar").clicked() {
                                remove = Some(**id);
                            }
                        });
                    });
                }
            });
    });
    if let Some(id) = remove {
        app.store.pair_mut(src, dst).issues.remove(&id);
        app.save();
    }
}

fn history(app: &mut App, ui: &mut Ui, src: &str, dst: &str) {
    let p = theme::palette(ui);
    let key = pair_key(src, dst);
    struct Batch {
        when: String,
        count: usize,
        hours: f64,
        undone: usize,
        from: String,
        to: String,
        dst_ids: Vec<u32>,
    }
    let mut batches: BTreeMap<String, Batch> = BTreeMap::new();
    for l in app.store.ledger.iter().filter(|l| l.pair == key) {
        let b = batches.entry(l.batch.clone()).or_insert_with(|| Batch {
            when: l.synced_at.clone(),
            count: 0,
            hours: 0.0,
            undone: 0,
            from: l.spent_on.clone(),
            to: l.spent_on.clone(),
            dst_ids: vec![],
        });
        b.count += 1;
        if l.undone {
            b.undone += 1;
        } else {
            b.hours += l.hours;
            b.dst_ids.push(l.dst_entry_id);
        }
        b.from = b.from.clone().min(l.spent_on.clone());
        b.to = b.to.clone().max(l.spent_on.clone());
    }
    let mut undo = None;
    theme::section_title(ui, icon::CLOCK_COUNTER_CLOCKWISE, "Historial de sincronizaciones");
    theme::card(ui, |ui| {
        if batches.is_empty() {
            ui.label(weak(ui, "Aún no has sincronizado nada en esta dirección."));
            return;
        }
        let can_undo = app.sessions.contains_key(dst);
        for (id, b) in batches.iter().rev().take(30) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::ROCKET_LAUNCH).color(p.accent));
                ui.label(RichText::new(&b.when).family(theme::semibold()));
                ui.label(weak(ui, format!(
                    "{} imputaciones · {} · {} – {}",
                    b.count,
                    theme::hours(b.hours),
                    dates::pretty(&b.from),
                    dates::pretty(&b.to)
                )));
                if b.undone == b.count {
                    theme::badge(ui, "Deshecha", p.text_weak);
                } else if b.undone > 0 {
                    theme::badge(ui, format!("{} deshechas", b.undone), p.warning);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !b.dst_ids.is_empty() {
                        let busy = app.mappings.undoing.is_some();
                        let enabled = can_undo && !busy;
                        ui.add_enabled_ui(enabled, |ui| {
                            if theme::danger_button(ui, format!("{}  Deshacer", icon::ARROW_COUNTER_CLOCKWISE))
                                .on_hover_text("Borra del destino las imputaciones creadas en esta sincronización")
                                .on_disabled_hover_text("Conecta el Redmine de destino para deshacer")
                                .clicked()
                            {
                                undo = Some((id.clone(), b.dst_ids.clone()));
                            }
                        });
                        if app.mappings.undoing.as_deref() == Some(id.as_str()) {
                            ui.spinner();
                        }
                    }
                });
            });
        }
    });
    if let Some((batch, ids)) = undo
        && let Some(client) = app.sessions.get(dst).map(|s| s.client.clone())
    {
        app.mappings.undoing = Some(batch.clone());
        app.worker.spawn(move || {
            let mut ok = vec![];
            let mut errors = vec![];
            for id in ids {
                match client.delete_time_entry(id) {
                    Ok(()) => ok.push(id),
                    // Ya no existe: se considera deshecha.
                    Err(crate::redmine::ApiError::Http { status: 404, .. }) => ok.push(id),
                    Err(e) => errors.push(format!("#{id}: {e}")),
                }
            }
            Msg::Undone { batch, ok, errors }
        });
    }
}
