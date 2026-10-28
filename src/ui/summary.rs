//! Resumen semanal: horas imputadas por día en cada Redmine, para detectar lo que falta.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate};
use egui::{Align, Layout, RichText, Sense, Ui, vec2};

use crate::app::{App, Msg, Tab, dates, weak};
use crate::redmine::{TimeEntry, TimeEntryQuery};
use crate::theme::{self, icon};

pub struct SummaryState {
    week: NaiveDate,
    seq: u64,
    loading: bool,
    loaded: Option<(NaiveDate, Vec<String>)>,
    data: BTreeMap<String, Result<Vec<TimeEntry>, String>>,
}

impl Default for SummaryState {
    fn default() -> Self {
        Self { week: dates::week_of(dates::today()).0, seq: 0, loading: false, loaded: None, data: BTreeMap::new() }
    }
}

pub fn on_weekly(app: &mut App, seq: u64, results: Vec<(String, Result<Vec<TimeEntry>, String>)>) {
    if seq != app.summary.seq {
        return;
    }
    app.summary.loading = false;
    app.summary.data = results.into_iter().collect();
}

fn load(app: &mut App, connected: Vec<String>) {
    let st = &mut app.summary;
    st.seq += 1;
    st.loading = true;
    st.loaded = Some((st.week, connected.clone()));
    let seq = st.seq;
    let from = dates::iso(st.week);
    let to = dates::iso(st.week + Duration::days(6));
    let clients: Vec<_> = connected
        .iter()
        .filter_map(|id| app.sessions.get(id).map(|s| (id.clone(), s.client.clone())))
        .collect();
    app.worker.spawn(move || {
        let handles: Vec<_> = clients
            .into_iter()
            .map(|(id, c)| {
                let q = TimeEntryQuery {
                    only_me: true,
                    from: Some(from.clone()),
                    to: Some(to.clone()),
                    ..Default::default()
                };
                std::thread::spawn(move || (id, c.time_entries(&q).map_err(|e| e.to_string())))
            })
            .collect();
        let results = handles.into_iter().filter_map(|h| h.join().ok()).collect();
        Msg::Weekly { seq, results }
    });
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let connected = app.connected_ids();
    if app.summary.loaded.as_ref() != Some(&(app.summary.week, connected.clone())) && !app.summary.loading {
        load(app, connected.clone());
    }
    let p = theme::palette(ui);
    crate::app::page(ui, "summary_page", |ui| {
        theme::page_header(
            ui,
            "Resumen semanal",
            "Tus horas por día en cada Redmine. Las diferencias indican imputaciones pendientes de trasladar.",
        );
        let week = app.summary.week;
        let mut nav = None;
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button(icon::CARET_LEFT).clicked() {
                    nav = Some(week - Duration::days(7));
                }
                ui.label(
                    RichText::new(format!(
                        "{} – {}",
                        week.format("%d/%m/%Y"),
                        (week + Duration::days(6)).format("%d/%m/%Y")
                    ))
                    .size(16.0)
                    .family(theme::semibold()),
                );
                if ui.button(icon::CARET_RIGHT).clicked() {
                    nav = Some(week + Duration::days(7));
                }
                if ui.button("Hoy").clicked() {
                    nav = Some(dates::week_of(dates::today()).0);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::icon_button(ui, icon::ARROW_CLOCKWISE, "Recargar").clicked() {
                        app.summary.loaded = None;
                    }
                    if app.summary.loading {
                        ui.spinner();
                    }
                });
            });
        });
        if let Some(w) = nav {
            app.summary.week = w;
        }
        ui.add_space(12.0);

        let mut per_day: BTreeMap<(String, String), f64> = BTreeMap::new();
        for (inst, res) in &app.summary.data {
            match res {
                Ok(entries) => {
                    for e in entries {
                        *per_day.entry((inst.clone(), e.spent_on.clone())).or_default() += e.hours;
                    }
                }
                Err(e) => theme::callout(ui, icon::WARNING, p.danger, format!("{}: {e}", app.store.instance_name(inst))),
            }
        }
        let max = per_day.values().cloned().fold(8.0, f64::max);
        let days: Vec<NaiveDate> = (0..7).map(|d| week + Duration::days(d)).collect();

        theme::card(ui, |ui| {
            egui::Grid::new("weekly")
                .num_columns(connected.len() + 2)
                .spacing(vec2(22.0, 12.0))
                .min_col_width(90.0)
                .show(ui, |ui| {
                    ui.label(weak(ui, "Día"));
                    for id in &connected {
                        ui.label(RichText::new(app.store.instance_name(id)).family(theme::semibold()));
                    }
                    ui.label(weak(ui, "Diferencia"));
                    ui.end_row();
                    let mut totals = vec![0.0; connected.len()];
                    for day in &days {
                        let iso = dates::iso(*day);
                        let weekend = day.weekday().num_days_from_monday() >= 5;
                        let label = format!("{} {}", dates::weekday_es(*day), day.format("%d/%m"));
                        ui.label(if weekend { weak(ui, label) } else { RichText::new(label) });
                        let hours: Vec<f64> = connected
                            .iter()
                            .map(|id| *per_day.get(&(id.clone(), iso.clone())).unwrap_or(&0.0))
                            .collect();
                        for (i, h) in hours.iter().enumerate() {
                            totals[i] += h;
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(vec2(90.0, 8.0), Sense::hover());
                                ui.painter().rect_filled(rect, 4, p.control);
                                let mut fill = rect;
                                fill.set_width(rect.width() * (*h / max).min(1.0) as f32);
                                ui.painter().rect_filled(fill, 4, p.accent);
                                ui.label(if *h > 0.0 { RichText::new(theme::hours(*h)) } else { weak(ui, "—") });
                            });
                        }
                        diff_cell(ui, &p, &hours);
                        ui.end_row();
                    }
                    ui.label(RichText::new("Total").family(theme::semibold()));
                    for t in &totals {
                        ui.label(RichText::new(theme::hours(*t)).family(theme::semibold()).size(16.0));
                    }
                    diff_cell(ui, &p, &totals);
                    ui.end_row();
                });
        });
        if connected.len() >= 2 {
            ui.add_space(10.0);
            if ui.button(format!("{}  Sincronizar esta semana", icon::ARROWS_CLOCKWISE)).clicked() {
                app.sync.set_range(week, week + Duration::days(6));
                app.tab = Tab::Sync;
            }
        }
    });
}

fn diff_cell(ui: &mut Ui, p: &theme::Palette, hours: &[f64]) {
    if hours.len() < 2 {
        ui.label("");
        return;
    }
    let max = hours.iter().cloned().fold(f64::MIN, f64::max);
    let min = hours.iter().cloned().fold(f64::MAX, f64::min);
    if max - min < 0.01 {
        ui.label(RichText::new(icon::CHECK).color(p.success));
    } else {
        theme::badge(ui, format!("± {}", theme::hours(max - min)), p.warning);
    }
}
