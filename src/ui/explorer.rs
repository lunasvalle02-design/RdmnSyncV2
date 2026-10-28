//! Explorador: proyectos del usuario, issues asignados con sus detalles e imputaciones.

use egui::{Align, Layout, Margin, RichText, Sense, TextEdit, Ui, vec2};
use egui_extras::{Column, TableBuilder};

use crate::app::{App, Loadable, Msg, dates, weak};
use crate::matching::normalize;
use crate::redmine::{Issue, IssueQuery, TimeEntry, TimeEntryQuery};
use crate::theme::{self, icon};

pub struct ExplorerState {
    pub inst: Option<String>,
    pub project: Option<u32>,
    project_filter: String,
    issue_filter: String,
    only_mine: bool,
    include_closed: bool,
    issues: Loadable<Vec<Issue>>,
    selected: Option<u32>,
    entries: Loadable<Vec<TimeEntry>>,
    only_my_entries: bool,
    seq: u64,
}

impl Default for ExplorerState {
    fn default() -> Self {
        Self {
            inst: None,
            project: None,
            project_filter: String::new(),
            issue_filter: String::new(),
            only_mine: true,
            include_closed: false,
            issues: Loadable::Idle,
            selected: None,
            entries: Loadable::Idle,
            only_my_entries: false,
            seq: 0,
        }
    }
}

pub fn on_issues(app: &mut App, seq: u64, result: Result<Vec<Issue>, String>) {
    if seq == app.explorer.seq {
        app.explorer.issues = result.map_or_else(Loadable::Failed, Loadable::Ready);
    }
}

pub fn on_entries(app: &mut App, seq: u64, result: Result<Vec<TimeEntry>, String>) {
    if seq == app.explorer.seq {
        app.explorer.entries = result.map_or_else(Loadable::Failed, Loadable::Ready);
    }
}

#[cfg(debug_assertions)]
pub fn dev_open_first(app: &mut App) {
    let inst = app.connected_ids().into_iter().next();
    app.explorer.project = inst.as_ref().and_then(|i| app.sessions[i].projects.first().map(|p| p.id));
    app.explorer.inst = inst;
    load_issues(app);
}

fn load_issues(app: &mut App) {
    let st = &mut app.explorer;
    let (Some(inst), Some(project)) = (&st.inst, st.project) else { return };
    let Some(session) = app.sessions.get(inst) else { return };
    st.seq += 1;
    st.issues = Loadable::Loading;
    st.selected = None;
    st.entries = Loadable::Idle;
    let seq = st.seq;
    let client = session.client.clone();
    let q = IssueQuery {
        project_id: Some(project),
        assigned_to_me: st.only_mine,
        include_closed: st.include_closed,
        limit: 500,
        ..Default::default()
    };
    app.worker.spawn(move || Msg::ExplorerIssues { seq, result: client.issues(&q).map_err(|e| e.to_string()) });
}

fn load_entries(app: &mut App) {
    let st = &mut app.explorer;
    let (Some(inst), Some(issue)) = (&st.inst, st.selected) else { return };
    let Some(session) = app.sessions.get(inst) else { return };
    st.seq += 1;
    st.entries = Loadable::Loading;
    let seq = st.seq;
    let client = session.client.clone();
    let q = TimeEntryQuery { issue_id: Some(issue), only_me: st.only_my_entries, ..Default::default() };
    app.worker
        .spawn(move || Msg::ExplorerEntries { seq, result: client.time_entries(&q).map_err(|e| e.to_string()) });
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui);
    let connected = app.connected_ids();
    if app.explorer.inst.as_ref().is_none_or(|i| !app.sessions.contains_key(i)) {
        app.explorer.inst = connected.first().cloned();
        app.explorer.project = None;
        app.explorer.issues = Loadable::Idle;
    }

    egui::Panel::left("explorer_projects")
        .resizable(true)
        .default_size(290.0)
        .size_range(220.0..=480.0)
        .frame(egui::Frame::new().fill(p.surface).inner_margin(Margin::same(14)))
        .show(ui, |ui| projects_panel(app, ui, &connected));

    if app.explorer.selected.is_some() {
        egui::Panel::right("explorer_detail")
            .resizable(true)
            .default_size(460.0)
            .size_range(340.0..=760.0)
            .frame(egui::Frame::new().fill(p.surface).inner_margin(Margin::same(16)))
            .show(ui, |ui| detail_panel(app, ui));
    }

    egui::CentralPanel::default()
        .frame(egui::Frame::new().inner_margin(Margin::symmetric(20, 16)))
        .show(ui, |ui| issues_panel(app, ui));
}

fn projects_panel(app: &mut App, ui: &mut Ui, connected: &[String]) {
    ui.label(weak(ui, "REDMINE").size(11.0));
    let mut changed_inst = None;
    ui.horizontal_wrapped(|ui| {
        for id in connected {
            let sel = app.explorer.inst.as_deref() == Some(id.as_str());
            if theme::pill(ui, sel, app.store.instance_name(id)).clicked() && !sel {
                changed_inst = Some(id.clone());
            }
        }
    });
    if let Some(id) = changed_inst {
        app.explorer.inst = Some(id);
        app.explorer.project = None;
        app.explorer.selected = None;
        app.explorer.issues = Loadable::Idle;
    }
    ui.add_space(6.0);
    ui.add(
        TextEdit::singleline(&mut app.explorer.project_filter)
            .hint_text(format!("{}  Buscar proyecto", icon::MAGNIFYING_GLASS))
            .desired_width(f32::INFINITY),
    );
    ui.add_space(4.0);
    let Some(session) = app.explorer.inst.as_ref().and_then(|i| app.sessions.get(i)) else { return };
    let filter = normalize(&app.explorer.project_filter);
    let ids: std::collections::HashSet<u32> = session.projects.iter().map(|p| p.id).collect();
    let mut clicked = None;
    ui.label(weak(ui, format!("PROYECTOS ({})", session.projects.len())).size(11.0));
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for project in &session.projects {
            if !filter.is_empty() && !normalize(&project.name).contains(&filter) {
                continue;
            }
            let selected = app.explorer.project == Some(project.id);
            let nested = project.parent.as_ref().is_some_and(|par| ids.contains(&par.id));
            let ic = if selected { icon::FOLDER_OPEN } else { icon::FOLDER };
            let indent = if nested && filter.is_empty() { 16.0 } else { 0.0 };
            let resp = theme::list_item(ui, selected, &format!("{ic}  {}", project.name), indent);
            let resp = match &project.parent {
                Some(parent) => resp.on_hover_text(format!("Subproyecto de {}", parent.name)),
                None => resp.on_hover_text(&project.name),
            };
            if resp.clicked() {
                clicked = Some(project.id);
            }
        }
    });
    if let Some(id) = clicked {
        app.explorer.project = Some(id);
        load_issues(app);
    }
}

fn issues_panel(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui);
    let Some(session) = app.explorer.inst.as_ref().and_then(|i| app.sessions.get(i)) else { return };
    let Some(project) = app.explorer.project.and_then(|id| session.projects.iter().find(|p| p.id == id)) else {
        theme::empty_state(ui, icon::FOLDERS, "Elige un proyecto", "Verás aquí sus issues y podrás consultar sus imputaciones.");
        return;
    };
    let project_name = project.name.clone();
    let base = session.client.base_url().to_string();

    theme::page_header(ui, &project_name, &format!("{} · {}", app.store.instance_name(app.explorer.inst.as_deref().unwrap_or_default()), base));
    let mut reload = false;
    ui.horizontal(|ui| {
        reload |= ui.toggle_value(&mut app.explorer.only_mine, format!("{}  Asignados a mí", icon::USER)).changed();
        reload |= ui.toggle_value(&mut app.explorer.include_closed, format!("{}  Incluir cerrados", icon::ARCHIVE)).changed();
        ui.add(
            TextEdit::singleline(&mut app.explorer.issue_filter)
                .hint_text(format!("{}  Filtrar por asunto o número", icon::FUNNEL))
                .desired_width(260.0),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if theme::icon_button(ui, icon::ARROW_CLOCKWISE, "Recargar").clicked() {
                reload = true;
            }
        });
    });
    if reload {
        load_issues(app);
        return;
    }
    ui.add_space(8.0);

    let issues = match &app.explorer.issues {
        Loadable::Idle | Loadable::Loading => {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.spinner();
            });
            return;
        }
        Loadable::Failed(e) => {
            theme::callout(ui, icon::WARNING, p.danger, e.clone());
            return;
        }
        Loadable::Ready(list) => list,
    };
    let filter = normalize(&app.explorer.issue_filter);
    let visible: Vec<&Issue> = issues
        .iter()
        .filter(|i| filter.is_empty() || normalize(&i.subject).contains(&filter) || i.id.to_string().contains(&filter))
        .collect();
    if visible.is_empty() {
        theme::empty_state(ui, icon::CHECK_SQUARE, "No hay issues", "Prueba a quitar filtros o incluir los cerrados.");
        return;
    }

    let mut clicked = None;
    let selected = app.explorer.selected;
    theme::card_frame(ui).inner_margin(Margin::same(8)).show(ui, |ui| {
        TableBuilder::new(ui)
            .striped(true)
            .sense(Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(64.0))
            .column(Column::auto().at_least(80.0))
            .column(Column::remainder().at_least(200.0).clip(true))
            .column(Column::auto().at_least(90.0))
            .column(Column::auto().at_least(70.0))
            .column(Column::exact(90.0))
            .header(26.0, |mut h| {
                for t in ["#", "Tipo", "Asunto", "Estado", "Prioridad", "Progreso"] {
                    h.col(|ui| {
                        ui.label(weak(ui, t).size(12.0));
                    });
                }
            })
            .body(|body| {
                body.rows(32.0, visible.len(), |mut row| {
                    let issue = visible[row.index()];
                    row.set_selected(selected == Some(issue.id));
                    row.col(|ui| {
                        ui.label(weak(ui, format!("#{}", issue.id)));
                    });
                    row.col(|ui| {
                        theme::badge(ui, &issue.tracker.name, p.accent);
                    });
                    row.col(|ui| {
                        ui.add(egui::Label::new(&issue.subject).truncate());
                    });
                    row.col(|ui| {
                        ui.label(&issue.status.name);
                    });
                    row.col(|ui| {
                        ui.label(weak(ui, &issue.priority.name));
                    });
                    row.col(|ui| {
                        let ratio = issue.done_ratio.unwrap_or(0.0) / 100.0;
                        ui.add(egui::ProgressBar::new(ratio).desired_width(80.0).desired_height(6.0).fill(p.accent));
                    });
                    if row.response().clicked() {
                        clicked = Some(issue.id);
                    }
                });
            });
    });
    ui.label(weak(ui, format!("{} issues", visible.len())).size(12.0));
    if let Some(id) = clicked {
        app.explorer.selected = Some(id);
        load_entries(app);
    }
}

fn detail_panel(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui);
    let Some(session) = app.explorer.inst.as_ref().and_then(|i| app.sessions.get(i)) else { return };
    let Loadable::Ready(issues) = &app.explorer.issues else { return };
    let Some(issue) = app.explorer.selected.and_then(|id| issues.iter().find(|i| i.id == id)).cloned() else {
        return;
    };
    let url = session.client.issue_url(issue.id);

    let mut close = false;
    let mut reload = false;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal(|ui| {
            theme::badge(ui, format!("{} #{}", issue.tracker.name, issue.id), p.accent);
            theme::badge(ui, &issue.status.name, p.info);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::icon_button(ui, icon::X, "Cerrar").clicked() {
                    close = true;
                }
                ui.hyperlink_to(format!("{}  Abrir", icon::ARROW_SQUARE_OUT), &url);
            });
        });
        ui.add_space(4.0);
        ui.label(RichText::new(&issue.subject).size(19.0).family(theme::semibold()));
        ui.add_space(8.0);

        theme::card(ui, |ui| {
            egui::Grid::new("issue_attrs").num_columns(4).spacing(vec2(14.0, 8.0)).show(ui, |ui| {
                let attr = |ui: &mut Ui, k: &str, v: String| {
                    ui.label(weak(ui, k));
                    ui.label(if v.is_empty() { "—".to_string() } else { v });
                };
                attr(ui, "Proyecto", issue.project.name.clone());
                attr(ui, "Prioridad", issue.priority.name.clone());
                ui.end_row();
                attr(ui, "Asignado a", issue.assigned_to.as_ref().map(|a| a.name.clone()).unwrap_or_default());
                attr(ui, "Autor", issue.author.as_ref().map(|a| a.name.clone()).unwrap_or_default());
                ui.end_row();
                attr(ui, "Inicio", issue.start_date.as_deref().map(dates::pretty).unwrap_or_default());
                attr(ui, "Fin", issue.due_date.as_deref().map(dates::pretty).unwrap_or_default());
                ui.end_row();
                attr(ui, "Estimado", issue.estimated_hours.map(theme::hours).unwrap_or_default());
                attr(ui, "Dedicado", issue.spent_hours.map(theme::hours).unwrap_or_default());
                ui.end_row();
                for cf in issue.custom_fields.iter().filter(|c| !c.value_text().is_empty()) {
                    attr(ui, &cf.name, cf.value_text());
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            let ratio = issue.done_ratio.unwrap_or(0.0);
            ui.add(egui::ProgressBar::new(ratio / 100.0).text(format!("{ratio:.0}% completado")).fill(p.accent));
        });

        if !issue.description_text().trim().is_empty() {
            ui.add_space(10.0);
            theme::section_title(ui, icon::TEXT_ALIGN_LEFT, "Descripción");
            theme::card(ui, |ui| {
                ui.add(egui::Label::new(issue.description_text()).wrap());
            });
        }

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            theme::section_title(ui, icon::CLOCK, "Imputaciones");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                reload |= ui.toggle_value(&mut app.explorer.only_my_entries, "Solo las mías").changed();
            });
        });
        match &app.explorer.entries {
            Loadable::Idle | Loadable::Loading => {
                ui.spinner();
            }
            Loadable::Failed(e) => theme::callout(ui, icon::WARNING, p.danger, e.clone()),
            Loadable::Ready(entries) if entries.is_empty() => {
                ui.label(weak(ui, "Sin imputaciones."));
            }
            Loadable::Ready(entries) => {
                let total: f64 = entries.iter().map(|e| e.hours).sum();
                theme::card(ui, |ui| {
                    TableBuilder::new(ui)
                        .id_salt("issue_entries")
                        .striped(true)
                        .cell_layout(Layout::left_to_right(Align::Center))
                        .column(Column::exact(84.0))
                        .column(Column::auto().at_least(90.0))
                        .column(Column::auto().at_least(90.0))
                        .column(Column::exact(60.0))
                        .column(Column::remainder().clip(true))
                        .header(24.0, |mut h| {
                            for t in ["Fecha", "Usuario", "Actividad", "Horas", "Comentario"] {
                                h.col(|ui| {
                                    ui.label(weak(ui, t).size(12.0));
                                });
                            }
                        })
                        .body(|mut body| {
                            for e in entries {
                                body.row(28.0, |mut row| {
                                    row.col(|ui| {
                                        ui.label(dates::pretty(&e.spent_on));
                                    });
                                    row.col(|ui| {
                                        ui.add(egui::Label::new(&e.user.name).truncate());
                                    });
                                    row.col(|ui| {
                                        theme::badge(ui, &e.activity.name, p.info);
                                    });
                                    row.col(|ui| {
                                        ui.label(RichText::new(theme::hours(e.hours)).family(theme::semibold()));
                                    });
                                    row.col(|ui| {
                                        ui.add(egui::Label::new(weak(ui, e.comments_text())).truncate())
                                            .on_hover_text(e.comments_text());
                                    });
                                });
                            }
                        });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(weak(ui, format!("{} imputaciones", entries.len())));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(format!("Total {}", theme::hours(total))).family(theme::semibold()));
                        });
                    });
                });
            }
        }
    });
    if close {
        app.explorer.selected = None;
    }
    if reload {
        load_entries(app);
    }
}
