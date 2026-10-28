//! Asistente de sincronización: análisis, resolución de dudas, creación de issues y envío.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Duration, Local};
use egui::{Align, Layout, Margin, RichText, TextEdit, Ui, vec2};
use egui_extras::{Column, TableBuilder};

use crate::app::{App, Loadable, Msg, ToastKind, dates, weak};
use crate::issue_sync::{self, DiffCtx, Field, IssueDiff, Pending};
use crate::matching::{self, Confidence};
use crate::redmine::{
    CustomFieldInput, CustomFieldValue, Issue, NewIssue, Priority, TimeEntry, Tracker,
};
use crate::store::{IssueLink, LedgerEntry, LinkOrigin, ProjectLink, VersionLink, pair_key};
use crate::sync::{
    self, ExecItem, IssueState, Plan, PlanInput, ProjectSuggestion, Readiness, RowSync, SyncData,
};
use crate::theme::{self, icon};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Preset {
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    Custom,
}

pub struct SyncState {
    src: Option<String>,
    dst: Option<String>,
    preset: Preset,
    from: String,
    to: String,
    src_project: Option<u32>,
    seq: u64,
    loading: bool,
    error: Option<String>,
    plan: Option<Plan>,
    exec: Option<ExecProgress>,
    create: Option<CreateDialog>,
    only_pending: bool,
    /// Sugerencias de proyecto descartadas en este análisis.
    dismissed_projects: HashSet<u32>,
    /// Issues de origen cuyo issue de destino NO se actualizará en este envío.
    issue_skip: HashSet<u32>,
    status_choice: BTreeMap<String, Option<String>>,
    version_choice: BTreeMap<u32, Option<(u32, String)>>,
}

struct ExecProgress {
    batch: String,
    total: usize,
    done: usize,
    ok: usize,
    failed: usize,
    hours: f64,
    issues_ok: usize,
    /// Avisos de issues: errores o campos que Redmine no aplicó.
    issue_warnings: Vec<String>,
    finished: bool,
}

impl Default for SyncState {
    fn default() -> Self {
        let (from, to) = dates::week_of(dates::today());
        Self {
            src: None,
            dst: None,
            preset: Preset::ThisWeek,
            from: dates::iso(from),
            to: dates::iso(to),
            src_project: None,
            seq: 0,
            loading: false,
            error: None,
            plan: None,
            exec: None,
            create: None,
            only_pending: false,
            dismissed_projects: HashSet::new(),
            issue_skip: HashSet::new(),
            status_choice: BTreeMap::new(),
            version_choice: BTreeMap::new(),
        }
    }
}

impl SyncState {
    pub fn set_range(&mut self, from: chrono::NaiveDate, to: chrono::NaiveDate) {
        self.preset = Preset::Custom;
        self.from = dates::iso(from);
        self.to = dates::iso(to);
    }

    fn apply_preset(&mut self, preset: Preset) {
        self.preset = preset;
        let today = dates::today();
        let range = match preset {
            Preset::ThisWeek => dates::week_of(today),
            Preset::LastWeek => dates::week_of(today - Duration::days(7)),
            Preset::ThisMonth => dates::month_of(today),
            Preset::LastMonth => dates::month_of(dates::month_of(today).0 - Duration::days(1)),
            Preset::Custom => return,
        };
        self.from = dates::iso(range.0);
        self.to = dates::iso(range.1);
    }
}

// ---- Mensajes ---------------------------------------------------------------

pub fn on_plan(app: &mut App, seq: u64, result: Result<SyncData, String>) {
    if seq != app.sync.seq {
        return;
    }
    app.sync.loading = false;
    let (Some(src), Some(dst)) = (app.sync.src.clone(), app.sync.dst.clone()) else { return };
    let data = match result {
        Ok(d) => d,
        Err(e) => {
            app.sync.error = Some(e);
            return;
        }
    };
    let (Some(s), Some(d)) = (app.sessions.get(&src), app.sessions.get(&dst)) else { return };
    let mappings = app.store.pair(&src, &dst);
    let key = pair_key(&src, &dst);
    let ledger = app.store.synced_map(&key);
    let plan = Plan::build(
        data,
        PlanInput {
            src_id: &src,
            src_base: s.client.base_url(),
            dst_base: d.client.base_url(),
            dst_user_id: d.user.id,
            dst_projects: &d.projects,
            mappings: &mappings,
            ledger: &ledger,
            settings: &app.store.settings,
        },
    );
    // Persistir los mapeos de actividades con nombre idéntico (no requieren confirmación).
    let new_same: Vec<(String, String)> = plan
        .activities
        .iter()
        .filter(|(_, a)| a.confirmed && a.is_new)
        .filter_map(|(k, a)| a.dst.clone().map(|d| (k.clone(), d)))
        .collect();
    if !new_same.is_empty() {
        let pair = app.store.pair_mut(&src, &dst);
        for (s, d) in new_same {
            pair.set_activity(&s, &d);
        }
        app.save();
    }
    app.sync.plan = Some(plan);
    #[cfg(debug_assertions)]
    if std::env::var("RDMNSYNC_DEV_CREATE").is_ok() {
        let first = app.sync.plan.as_ref().and_then(|p| {
            p.issues.iter().find(|(_, r)| matches!(r.state, IssueState::Pending(Confidence::NotFound))).map(|(id, _)| *id)
        });
        if let Some(id) = first {
            open_create(app, id);
        }
    }
}

pub fn on_manual_issue(app: &mut App, src_issue: u32, result: Result<Issue, String>) {
    match result {
        Ok(issue) => {
            confirm_link(app, src_issue, &issue, LinkOrigin::Confirmed);
            app.toast(ToastKind::Success, format!("Enlazado con #{} {}", issue.id, issue.subject));
        }
        Err(e) => app.toast(ToastKind::Error, format!("No se pudo leer el issue: {e}")),
    }
}

pub fn on_create_meta(app: &mut App, project: u32, result: Result<(Vec<Tracker>, Vec<Priority>), String>) {
    let mappings = match (&app.sync.src, &app.sync.dst) {
        (Some(s), Some(d)) => app.store.pair(s, d),
        _ => return,
    };
    let Some(dlg) = app.sync.create.as_mut() else { return };
    if dlg.project != Some(project) {
        return;
    }
    match result {
        Ok((trackers, priorities)) => {
            let tnames: Vec<String> = trackers.iter().map(|t| t.name.clone()).collect();
            let wanted = mappings.trackers.get(&dlg.src.tracker.name).cloned();
            dlg.tracker = wanted
                .and_then(|w| trackers.iter().find(|t| t.name == w))
                .or_else(|| matching::suggest_name(&dlg.src.tracker.name, &tnames).map(|(i, _)| &trackers[i]))
                .or(trackers.first())
                .map(|t| t.id);
            let pnames: Vec<String> = priorities.iter().map(|p| p.name.clone()).collect();
            let wanted = mappings.priorities.get(&dlg.src.priority.name).cloned();
            dlg.priority = wanted
                .and_then(|w| priorities.iter().find(|p| p.name == w))
                .or_else(|| {
                    matching::suggest_name(&dlg.src.priority.name, &pnames)
                        .filter(|(_, s)| *s >= 0.6)
                        .map(|(i, _)| &priorities[i])
                })
                .or_else(|| priorities.iter().find(|p| p.is_default))
                .map(|p| p.id);
            dlg.trackers = Loadable::Ready(trackers);
            dlg.priorities = priorities;
            dlg.fields = Loadable::Idle;
        }
        Err(e) => dlg.trackers = Loadable::Failed(e),
    }
}

pub fn on_custom_fields(app: &mut App, project: u32, tracker: u32, result: Result<Vec<CustomFieldValue>, String>) {
    let Some(dlg) = app.sync.create.as_mut() else { return };
    if dlg.project != Some(project) || dlg.tracker != Some(tracker) {
        return;
    }
    dlg.fields = match result {
        Ok(fields) => Loadable::Ready(
            fields
                .into_iter()
                .map(|f| {
                    let value = dlg
                        .src
                        .custom_fields
                        .iter()
                        .find(|s| matching::same_name(&s.name, &f.name))
                        .map(|s| s.value_text())
                        .unwrap_or_default();
                    (f, value)
                })
                .collect(),
        ),
        Err(e) => Loadable::Failed(e),
    };
}

pub fn on_issue_created(app: &mut App, src_issue: u32, result: Result<Issue, String>) {
    match result {
        Ok(issue) => {
            if let (Some(dlg), Some(s), Some(d)) = (&app.sync.create, &app.sync.src, &app.sync.dst) {
                let tracker = match &dlg.trackers {
                    Loadable::Ready(t) => t.iter().find(|t| Some(t.id) == dlg.tracker).map(|t| t.name.clone()),
                    _ => None,
                };
                let priority = dlg.priorities.iter().find(|p| Some(p.id) == dlg.priority).map(|p| p.name.clone());
                let (src_tracker, src_priority, src_project) =
                    (dlg.src.tracker.name.clone(), dlg.src.priority.name.clone(), dlg.src.project.clone());
                let (s, d) = (s.clone(), d.clone());
                let pair = app.store.pair_mut(&s, &d);
                if let Some(t) = tracker {
                    pair.trackers.insert(src_tracker, t);
                }
                if let Some(p) = priority {
                    pair.priorities.insert(src_priority, p);
                }
                pair.projects.entry(src_project.id).or_insert(ProjectLink {
                    src_name: src_project.name,
                    dst_id: issue.project.id,
                    dst_name: issue.project.name.clone(),
                });
            }
            app.sync.create = None;
            confirm_link(app, src_issue, &issue, LinkOrigin::Created);
            app.toast(ToastKind::Success, format!("Issue #{} creado en el destino", issue.id));
        }
        Err(e) => {
            if let Some(dlg) = app.sync.create.as_mut() {
                dlg.submitting = false;
                dlg.error = Some(e);
            }
        }
    }
}

pub fn on_pushed(app: &mut App, item: ExecItem, result: Result<TimeEntry, String>) {
    let (Some(src), Some(dst)) = (app.sync.src.clone(), app.sync.dst.clone()) else { return };
    let Some(exec) = app.sync.exec.as_mut() else { return };
    exec.done += 1;
    let Some(plan) = app.sync.plan.as_mut() else { return };
    match result {
        Ok(created) => {
            exec.ok += 1;
            exec.hours += created.hours;
            let batch = exec.batch.clone();
            if let Some(row) = plan.rows.get_mut(item.row) {
                row.sync = RowSync::Done { dst_entry_id: created.id };
                row.include = false;
            }
            plan.data.dst_entries.push(created.clone());
            // Los enlaces automáticos usados con éxito se guardan para la próxima vez.
            let auto_link = item.src_issue_id.and_then(|sid| match plan.issues.get(&sid).map(|r| (&r.state, &r.src)) {
                Some((IssueState::Linked { dst_id, subject, project_id, origin: LinkOrigin::Auto, .. }, src_issue)) => {
                    Some((sid, IssueLink {
                        src_subject: src_issue.subject.clone(),
                        dst_id: *dst_id,
                        dst_subject: subject.clone(),
                        dst_project_id: *project_id,
                        origin: LinkOrigin::Auto,
                        synced: Default::default(),
                    }))
                }
                _ => None,
            });
            if let Some((sid, link)) = auto_link {
                app.store.pair_mut(&src, &dst).issues.entry(sid).or_insert(link);
            }
            app.store.ledger.push(LedgerEntry {
                pair: pair_key(&src, &dst),
                batch,
                src_entry_id: item.src_entry_id,
                dst_entry_id: created.id,
                src_issue_id: item.src_issue_id,
                dst_issue_id: item.new.issue_id,
                hours: created.hours,
                spent_on: created.spent_on,
                activity: item.activity_name,
                synced_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                undone: false,
            });
        }
        Err(e) => {
            exec.failed += 1;
            if let Some(row) = plan.rows.get_mut(item.row) {
                row.sync = RowSync::Failed(e);
            }
        }
    }
}

pub fn on_exec_finished(app: &mut App) {
    app.save();
    let Some(exec) = app.sync.exec.as_mut() else { return };
    exec.finished = true;
    let (ok, failed, hours, issues_ok) = (exec.ok, exec.failed, exec.hours, exec.issues_ok);
    let warnings = exec.issue_warnings.clone();
    let mut parts = Vec::new();
    if ok + failed > 0 {
        parts.push(format!("{ok} imputaciones creadas ({})", theme::hours(hours)));
    }
    if issues_ok > 0 {
        parts.push(plural(issues_ok, "issue de destino actualizado", "issues de destino actualizados"));
    }
    let text = parts.join(" · ");
    if failed == 0 && warnings.is_empty() {
        app.toast(ToastKind::Success, text);
    } else {
        if failed > 0 {
            app.toast(ToastKind::Warning, format!("{text}. {failed} imputaciones con error: revisa la tabla."));
        }
        for w in warnings {
            app.toast(ToastKind::Warning, w);
        }
    }
}

pub fn on_issue_updated(
    app: &mut App,
    src_issue: u32,
    mut snapshot: BTreeMap<String, String>,
    auto_versions: Vec<(u32, String, u32, String)>,
    result: Result<(Issue, Vec<String>), String>,
) {
    let (Some(src), Some(dst)) = (app.sync.src.clone(), app.sync.dst.clone()) else { return };
    let Some(exec) = app.sync.exec.as_mut() else { return };
    exec.done += 1;
    let Some(plan) = app.sync.plan.as_mut() else { return };
    match result {
        Ok((issue, ignored)) => {
            exec.issues_ok += 1;
            if !ignored.is_empty() {
                exec.issue_warnings.push(format!(
                    "Redmine no aplicó en #{}: {} (flujo de trabajo, permisos o ajustes del destino)",
                    issue.id,
                    ignored.join(", ")
                ));
                // Lo no aplicado no se recuerda como sincronizado, para volver a intentarlo.
                snapshot.retain(|k, _| !Field::ALL.iter().any(|f| f.key() == k && ignored.iter().any(|l| l == f.label())));
            }
            if let Some(slot) = plan.data.dst_pool.iter_mut().find(|i| i.id == issue.id) {
                *slot = issue.clone();
            }
            let src_subject = plan.issues.get(&src_issue).map(|r| r.src.subject.clone()).unwrap_or_default();
            let origin = match plan.issues.get(&src_issue).map(|r| &r.state) {
                Some(IssueState::Linked { origin, .. }) => *origin,
                _ => LinkOrigin::Auto,
            };
            let pair = app.store.pair_mut(&src, &dst);
            let link = pair.issues.entry(src_issue).or_insert_with(|| IssueLink {
                src_subject,
                dst_id: issue.id,
                dst_subject: issue.subject.clone(),
                dst_project_id: issue.project.id,
                origin,
                synced: BTreeMap::new(),
            });
            link.synced.extend(snapshot);
            for (sid, sname, did, dname) in auto_versions {
                pair.versions.entry(sid).or_insert(VersionLink {
                    src_name: sname,
                    src_project: String::new(),
                    dst_id: did,
                    dst_name: dname,
                    dst_project: issue.project.name.clone(),
                });
            }
        }
        Err(e) => exec.issue_warnings.push(format!("No se pudo actualizar el issue de destino de #{src_issue}: {e}")),
    }
}

/// Diferencias entre cada issue de origen enlazado y su issue de destino.
/// Pide en segundo plano las versiones de destino que falten.
fn issue_diffs(app: &mut App) -> Vec<IssueDiff> {
    if !app.store.settings.update_issues {
        return vec![];
    }
    let (Some(src), Some(dst)) = (app.sync.src.clone(), app.sync.dst.clone()) else { return vec![] };
    let diffs = {
        let (Some(plan), Some(session)) = (&app.sync.plan, app.sessions.get(&dst)) else { return vec![] };
        let pair = app.store.pair(&src, &dst);
        let dst_versions: HashMap<u32, Vec<crate::redmine::Version>> = app
            .versions
            .iter()
            .filter(|((inst, _), _)| *inst == dst)
            .filter_map(|((_, p), v)| match v {
                Loadable::Ready(list) => Some((*p, list.clone())),
                _ => None,
            })
            .collect();
        let ctx = DiffCtx {
            pair: &pair,
            fields: &app.store.settings.issue_fields,
            respect_dst_changes: app.store.settings.respect_dst_changes,
            dst_statuses: &session.statuses,
            dst_priorities: &session.priorities,
            dst_versions: &dst_versions,
        };
        plan.issues
            .iter()
            .filter_map(|(id, res)| match &res.state {
                IssueState::Linked { dst_id, .. } => {
                    let target = plan.data.dst_pool.iter().find(|i| i.id == *dst_id)?;
                    Some(issue_sync::diff(&res.src, target, pair.issues.get(id).map(|l| &l.synced), &ctx))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for d in &diffs {
        for p in &d.pending {
            if let Pending::VersionsLoading(project) = p {
                app.request_versions(&dst, *project);
            }
        }
    }
    diffs
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn confirm_link(app: &mut App, src_issue: u32, issue: &Issue, origin: LinkOrigin) {
    let (Some(src), Some(dst)) = (app.sync.src.clone(), app.sync.dst.clone()) else { return };
    let Some(plan) = app.sync.plan.as_mut() else { return };
    let src_subject = plan.issues.get(&src_issue).map(|r| r.src.subject.clone()).unwrap_or_default();
    plan.link_issue(src_issue, issue, origin);
    app.store.pair_mut(&src, &dst).issues.insert(
        src_issue,
        IssueLink {
            src_subject,
            dst_id: issue.id,
            dst_subject: issue.subject.clone(),
            dst_project_id: issue.project.id,
            origin,
            synced: Default::default(),
        },
    );
    app.save();
}

// ---- Acciones ---------------------------------------------------------------

#[cfg(debug_assertions)]
pub fn dev_analyze(app: &mut App) {
    let ids = app.connected_ids();
    app.sync.src = ids.first().cloned();
    app.sync.dst = ids.get(1).cloned();
    analyze(app);
}

fn analyze(app: &mut App) {
    let st = &mut app.sync;
    let (Some(src), Some(dst)) = (&st.src, &st.dst) else { return };
    let (Some(s), Some(d)) = (app.sessions.get(src), app.sessions.get(dst)) else { return };
    let (Some(from), Some(to)) = (dates::parse(&st.from), dates::parse(&st.to)) else {
        st.error = Some("Las fechas deben tener el formato AAAA-MM-DD".into());
        return;
    };
    if from > to {
        st.error = Some("La fecha de inicio es posterior a la de fin".into());
        return;
    }
    st.seq += 1;
    st.loading = true;
    st.error = None;
    st.plan = None;
    st.exec = None;
    st.dismissed_projects.clear();
    st.issue_skip.clear();
    let seq = st.seq;
    let req = sync::LoadRequest {
        src: s.client.clone(),
        dst: d.client.clone(),
        from: dates::iso(from),
        to: dates::iso(to),
        src_project: st.src_project,
        mappings: app.store.pair(src, dst),
    };
    app.worker.spawn(move || Msg::PlanLoaded { seq, result: sync::load(req) });
}

fn execute(app: &mut App, diffs: &[IssueDiff]) {
    let Some(plan) = &app.sync.plan else { return };
    let Some(dst) = app.sync.dst.as_ref().and_then(|d| app.sessions.get(d)) else { return };
    let items = plan.build_items(app.store.settings.add_marker);
    let updates: Vec<IssueDiff> = diffs
        .iter()
        .filter(|d| !d.changes.is_empty() && !app.sync.issue_skip.contains(&d.src_id))
        .cloned()
        .collect();
    if items.is_empty() && updates.is_empty() {
        return;
    }
    app.sync.exec = Some(ExecProgress {
        batch: Local::now().format("%Y%m%d-%H%M%S").to_string(),
        total: items.len() + updates.len(),
        done: 0,
        ok: 0,
        failed: 0,
        hours: 0.0,
        issues_ok: 0,
        issue_warnings: Vec::new(),
        finished: false,
    });
    let client = dst.client.clone();
    app.worker.run(move |out| {
        // Primero las imputaciones: si se cerrase antes el issue, Redmine podría rechazarlas.
        sync::execute(&client, items, |item, result| out.send(Msg::Pushed { item: item.clone(), result }));
        // Después se igualan los issues de DESTINO; el origen solo se ha leído.
        for d in updates {
            let result = issue_sync::apply(&client, d.dst_id, &d.changes)
                .map(|(issue, ignored)| (issue, ignored.into_iter().map(String::from).collect()));
            out.send(Msg::IssueUpdated {
                src_issue: d.src_id,
                snapshot: d.snapshot,
                auto_versions: d.auto_versions,
                result,
            });
        }
        out.send(Msg::ExecFinished);
    });
}

fn issue_updates(app: &mut App, ui: &mut Ui, diffs: &[IssueDiff]) {
    if !app.store.settings.update_issues || diffs.is_empty() {
        return;
    }
    let p = theme::palette(ui);
    let (Some(src), Some(dst)) = (app.sync.src.clone(), app.sync.dst.clone()) else { return };
    let Some(plan) = &app.sync.plan else { return };
    let dst_statuses: Vec<String> =
        app.sessions.get(&dst).map(|s| s.statuses.iter().map(|x| x.name.clone()).collect()).unwrap_or_default();

    let mut statuses: Vec<String> = Vec::new();
    let mut versions: Vec<(u32, String, u32)> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut loading = false;
    for pend in diffs.iter().flat_map(|d| &d.pending) {
        match pend {
            Pending::Status(s) if !statuses.contains(s) => statuses.push(s.clone()),
            Pending::Version { src_id, src_name, dst_project } if !versions.iter().any(|v| v.0 == *src_id) => {
                versions.push((*src_id, src_name.clone(), *dst_project))
            }
            Pending::VersionsLoading(_) => loading = true,
            Pending::Note(n) if !notes.contains(n) => notes.push(n.clone()),
            _ => {}
        }
    }
    let version_options: HashMap<u32, Vec<crate::redmine::Version>> =
        versions.iter().map(|v| (v.2, app.versions_of(&dst, v.2).to_vec())).collect();
    let changed: Vec<&IssueDiff> = diffs.iter().filter(|d| !d.changes.is_empty()).collect();
    let up_to_date = diffs.len() - changed.len();
    let busy = app.sync.exec.as_ref().is_some_and(|e| !e.finished);

    let mut status_ok: Vec<(String, String)> = Vec::new();
    let mut version_ok: Vec<(u32, String, u32, String, u32)> = Vec::new();
    let mut toggles: Vec<(u32, bool)> = Vec::new();
    ui.horizontal(|ui| {
        theme::section_title(ui, icon::NOTE_PENCIL, "Actualizar issues de destino");
        if !changed.is_empty() {
            theme::badge(ui, format!("{} con cambios", changed.len()), p.accent);
        }
        if !statuses.is_empty() || !versions.is_empty() {
            theme::badge(ui, "Requiere tu atención", p.warning);
        }
    });
    theme::card(ui, |ui| {
        ui.label(weak(
            ui,
            format!(
                "Los issues de {} se igualarán con los datos de sus issues de {}. El origen solo se lee; nunca se modifica. Los campos se eligen en Ajustes.",
                app.store.instance_name(&dst),
                app.store.instance_name(&src)
            ),
        ));
        if !statuses.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Estados sin equivalente en el destino").family(theme::semibold()));
            egui::Grid::new("status_map").num_columns(4).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
                for name in &statuses {
                    let choice = app.sync.status_choice.entry(name.clone()).or_insert_with(|| {
                        matching::suggest_name(name, &dst_statuses).filter(|(_, s)| *s >= 0.5).map(|(i, _)| dst_statuses[i].clone())
                    });
                    ui.label(RichText::new(name).family(theme::semibold()));
                    ui.label(RichText::new(icon::ARROW_RIGHT).color(p.text_weak));
                    egui::ComboBox::from_id_salt(("status_map", name.as_str()))
                        .selected_text(choice.clone().unwrap_or_else(|| "Elegir…".into()))
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for d in &dst_statuses {
                                ui.selectable_value(choice, Some(d.clone()), d);
                            }
                        });
                    ui.add_enabled_ui(choice.is_some(), |ui| {
                        if ui.button(format!("{}  Confirmar", icon::CHECK)).clicked()
                            && let Some(d) = choice.clone()
                        {
                            status_ok.push((name.clone(), d));
                        }
                    });
                    ui.end_row();
                }
            });
        }
        if !versions.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Versiones sin equivalente en el destino").family(theme::semibold()));
            egui::Grid::new("version_map").num_columns(4).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
                for (src_id, src_name, project) in &versions {
                    let options = version_options.get(project).cloned().unwrap_or_default();
                    let choice = app.sync.version_choice.entry(*src_id).or_default();
                    ui.label(RichText::new(src_name).family(theme::semibold()));
                    ui.label(RichText::new(icon::ARROW_RIGHT).color(p.text_weak));
                    egui::ComboBox::from_id_salt(("version_map", *src_id))
                        .selected_text(choice.as_ref().map(|c| c.1.clone()).unwrap_or_else(|| "Elegir…".into()))
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for v in &options {
                                ui.selectable_value(choice, Some((v.id, v.name.clone())), &v.name);
                            }
                        });
                    ui.add_enabled_ui(choice.is_some(), |ui| {
                        if ui.button(format!("{}  Confirmar", icon::CHECK)).clicked()
                            && let Some((did, dname)) = choice.clone()
                        {
                            version_ok.push((*src_id, src_name.clone(), did, dname, *project));
                        }
                    });
                    ui.end_row();
                }
            });
            ui.label(weak(ui, "Si la versión no existe todavía, créala en el destino y pulsa «Analizar» de nuevo."));
        }
        if loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(weak(ui, "Cargando versiones del destino…"));
            });
        }
        for n in &notes {
            ui.label(RichText::new(format!("{}  {n}", icon::INFO)).color(p.text_weak));
        }
        if !changed.is_empty() {
            ui.add_space(6.0);
            ui.separator();
        }
        for d in &changed {
            let res = plan.issues.get(&d.src_id);
            let dst_subject = match res.map(|r| &r.state) {
                Some(IssueState::Linked { subject, .. }) => subject.clone(),
                _ => String::new(),
            };
            ui.horizontal(|ui| {
                let mut include = !app.sync.issue_skip.contains(&d.src_id);
                if ui.add_enabled(!busy, egui::Checkbox::without_text(&mut include)).changed() {
                    toggles.push((d.src_id, include));
                }
                ui.label(weak(ui, format!("#{}", d.src_id)));
                ui.label(RichText::new(icon::ARROW_RIGHT).color(p.text_weak));
                ui.hyperlink_to(format!("#{}", d.dst_id), format!("{}/issues/{}", plan.dst_base, d.dst_id));
                ui.add(egui::Label::new(RichText::new(dst_subject).family(theme::semibold())).truncate());
            });
            ui.horizontal_wrapped(|ui| {
                ui.add_space(30.0);
                for c in &d.changes {
                    egui::Frame::new()
                        .fill(p.control)
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(weak(ui, format!("{}:", c.field.label())).size(12.5));
                                ui.label(RichText::new(&c.from).size(12.5).strikethrough().color(p.text_weak));
                                ui.label(RichText::new(icon::ARROW_RIGHT).size(12.0).color(p.text_weak));
                                ui.label(RichText::new(&c.to).size(12.5).color(p.accent));
                            });
                        });
                }
            });
            ui.add_space(4.0);
        }
        if up_to_date > 0 {
            ui.label(weak(
                ui,
                format!("{}  {}", icon::CHECK, plural(up_to_date, "issue de destino ya está al día.", "issues de destino ya están al día.")),
            ));
        }
    });
    for (id, include) in toggles {
        if include {
            app.sync.issue_skip.remove(&id);
        } else {
            app.sync.issue_skip.insert(id);
        }
    }
    if !status_ok.is_empty() || !version_ok.is_empty() {
        let dst_names: HashMap<u32, String> = app
            .sessions
            .get(&dst)
            .map(|s| s.projects.iter().map(|p| (p.id, p.name.clone())).collect())
            .unwrap_or_default();
        let pair = app.store.pair_mut(&src, &dst);
        for (s, d) in status_ok {
            pair.set_status(&s, &d);
        }
        for (sid, sname, did, dname, project) in version_ok {
            pair.versions.insert(
                sid,
                VersionLink {
                    src_name: sname,
                    src_project: String::new(),
                    dst_id: did,
                    dst_name: dname,
                    dst_project: dst_names.get(&project).cloned().unwrap_or_default(),
                },
            );
        }
        app.save();
    }
    ui.add_space(8.0);
}

// ---- Vista ------------------------------------------------------------------

pub fn show(app: &mut App, ui: &mut Ui) {
    let connected = app.connected_ids();
    if app.sync.src.as_ref().is_none_or(|s| !app.sessions.contains_key(s)) {
        app.sync.src = connected.first().cloned();
    }
    if app.sync.dst.as_ref().is_none_or(|d| !app.sessions.contains_key(d) || Some(d) == app.sync.src.as_ref()) {
        app.sync.dst = connected.iter().find(|c| Some(*c) != app.sync.src.as_ref()).cloned();
    }

    let diffs = issue_diffs(app);
    crate::app::page(ui, "sync_page", |ui| {
        theme::page_header(
            ui,
            "Sincronizar imputaciones",
            "Analiza tus imputaciones del Redmine de origen y créalas en el de destino.",
        );
        if connected.len() < 2 {
            let p = theme::palette(ui);
            theme::callout(ui, icon::PLUGS, p.warning, "Necesitas al menos dos Redmines conectados. Usa el botón de conexiones de la barra superior.");
            return;
        }
        config_card(app, ui, &connected);
        ui.add_space(14.0);

        let p = theme::palette(ui);
        if let Some(e) = app.sync.error.clone() {
            theme::callout(ui, icon::WARNING_OCTAGON, p.danger, e);
        }
        if app.sync.loading {
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                ui.spinner();
                ui.label(weak(ui, "Leyendo imputaciones y buscando issues equivalentes…"));
            });
            return;
        }
        if app.sync.plan.is_none() {
            theme::empty_state(
                ui,
                icon::ARROWS_LEFT_RIGHT,
                "Listo para analizar",
                "Elige origen, destino y periodo, y pulsa «Analizar».",
            );
            return;
        }
        stats(app, ui);
        ui.add_space(12.0);
        pending_activities(app, ui);
        pending_projects(app, ui);
        pending_issues(app, ui);
        issue_updates(app, ui, &diffs);
        rows_table(app, ui);
        ui.add_space(12.0);
        footer(app, ui, &diffs);
    });

    create_dialog(app, ui);
}

fn config_card(app: &mut App, ui: &mut Ui, connected: &[String]) {
    let busy = app.sync.exec.as_ref().is_some_and(|e| !e.finished);
    let mut run = false;
    theme::card(ui, |ui| {
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                ui.label(weak(ui, "Origen"));
                instance_combo(ui, "sync_src", &mut app.sync.src, connected, &app.store);
                if theme::icon_button(ui, icon::ARROWS_LEFT_RIGHT, "Intercambiar origen y destino").clicked() {
                    std::mem::swap(&mut app.sync.src, &mut app.sync.dst);
                    app.sync.plan = None;
                    app.sync.src_project = None;
                }
                ui.label(weak(ui, "Destino"));
                let before = app.sync.dst.clone();
                instance_combo(ui, "sync_dst", &mut app.sync.dst, connected, &app.store);
                if app.sync.dst != before && app.sync.dst == app.sync.src {
                    app.sync.src = before;
                }
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(weak(ui, "Periodo"));
                for (preset, label) in [
                    (Preset::ThisWeek, "Esta semana"),
                    (Preset::LastWeek, "Semana pasada"),
                    (Preset::ThisMonth, "Este mes"),
                    (Preset::LastMonth, "Mes pasado"),
                ] {
                    if theme::pill(ui, app.sync.preset == preset, label).clicked() {
                        app.sync.apply_preset(preset);
                    }
                }
                ui.add_space(8.0);
                ui.label(icon::CALENDAR_BLANK);
                let a = ui.add(TextEdit::singleline(&mut app.sync.from).desired_width(104.0));
                ui.label("→");
                let b = ui.add(TextEdit::singleline(&mut app.sync.to).desired_width(104.0));
                if a.changed() || b.changed() {
                    app.sync.preset = Preset::Custom;
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(weak(ui, "Proyecto de origen"));
                let projects = app.sync.src.as_ref().and_then(|s| app.sessions.get(s)).map(|s| &s.projects);
                let selected = app.sync.src_project
                    .and_then(|id| projects.and_then(|ps| ps.iter().find(|p| p.id == id)))
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "Todos los proyectos".into());
                egui::ComboBox::from_id_salt("sync_src_project")
                    .selected_text(selected)
                    .width(300.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut app.sync.src_project, None, "Todos los proyectos");
                        for p in projects.into_iter().flatten() {
                            ui.selectable_value(&mut app.sync.src_project, Some(p.id), &p.name);
                        }
                    });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::primary_button(ui, format!("{}  Analizar", icon::MAGNIFYING_GLASS)).clicked() {
                        run = true;
                    }
                });
            });
        });
    });
    if run {
        analyze(app);
    }
}

fn instance_combo(ui: &mut Ui, id: &str, value: &mut Option<String>, options: &[String], store: &crate::store::Store) {
    let text = value.as_ref().map(|v| store.instance_name(v)).unwrap_or_default();
    egui::ComboBox::from_id_salt(id).selected_text(text).width(200.0).show_ui(ui, |ui| {
        for o in options {
            ui.selectable_value(value, Some(o.clone()), store.instance_name(o));
        }
    });
}

fn stats(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui);
    let Some(plan) = &app.sync.plan else { return };
    let total_h: f64 = plan.rows.iter().map(|r| r.entry.hours).sum();
    let (mut ready, mut ready_h, mut blocked, mut done) = (0, 0.0, 0, 0);
    for r in &plan.rows {
        match plan.readiness(r) {
            Readiness::Finished => done += 1,
            Readiness::Ready { .. } if r.include => {
                ready += 1;
                ready_h += r.entry.hours;
            }
            Readiness::Blocked(_) if r.include => blocked += 1,
            _ => {}
        }
    }
    ui.horizontal(|ui| {
        theme::stat(ui, icon::CLOCK, &format!("imputaciones en origen · {}", theme::hours(total_h)), plan.rows.len().to_string(), p.accent);
        theme::stat(ui, icon::CHECK_CIRCLE, &format!("listas para enviar · {}", theme::hours(ready_h)), ready.to_string(), p.success);
        theme::stat(ui, icon::QUESTION, "requieren tu atención", blocked.to_string(), p.warning);
        theme::stat(ui, icon::LINK, "ya sincronizadas", done.to_string(), p.info);
    });
}

fn pending_activities(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui);
    let (Some(src), Some(dst)) = (app.sync.src.clone(), app.sync.dst.clone()) else { return };
    let Some(plan) = app.sync.plan.as_mut() else { return };
    if plan.activities.is_empty() {
        return;
    }
    let pending = plan.activities.values().filter(|a| !a.confirmed).count();
    let dst_names: Vec<String> = plan.data.dst_activities.iter().map(|a| a.name.clone()).collect();
    let mut confirmed = Vec::new();
    let id = ui.make_persistent_id("acts_open");
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, pending > 0)
        .show_header(ui, |ui| {
            theme::section_title(ui, icon::TAG, "Tipos de imputación (actividades)");
            if pending > 0 {
                theme::badge(ui, format!("{pending} por confirmar"), p.warning);
            } else {
                theme::badge(ui, "Todo mapeado", p.success);
            }
        })
        .body(|ui| {
            theme::card(ui, |ui| {
                ui.label(weak(ui, format!(
                    "Cada Redmine nombra las actividades a su manera. Indica a qué actividad de {} corresponde cada una de {}; se recordará para esta pareja.",
                    app.store.instance_name(&dst), app.store.instance_name(&src)
                )));
                let suggested = plan.activities.values().filter(|a| !a.confirmed && a.dst.is_some()).count();
                if suggested > 1 && ui.button(format!("{}  Confirmar las {suggested} sugerencias", icon::CHECKS)).clicked() {
                    for (name, res) in plan.activities.iter_mut().filter(|(_, a)| !a.confirmed) {
                        if let Some(d) = &res.dst {
                            res.confirmed = true;
                            confirmed.push((name.clone(), d.clone()));
                        }
                    }
                }
                ui.add_space(6.0);
                egui::Grid::new("acts_grid").num_columns(4).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
                    for (name, res) in plan.activities.iter_mut() {
                        ui.label(RichText::new(name).family(theme::semibold()));
                        ui.label(RichText::new(icon::ARROW_RIGHT).color(p.text_weak));
                        let before = res.dst.clone();
                        egui::ComboBox::from_id_salt(("act", name.as_str()))
                            .selected_text(res.dst.clone().unwrap_or_else(|| "Elegir…".into()))
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                for d in &dst_names {
                                    ui.selectable_value(&mut res.dst, Some(d.clone()), d);
                                }
                            });
                        let changed = res.dst != before;
                        if res.confirmed {
                            if changed {
                                if let Some(d) = &res.dst {
                                    confirmed.push((name.clone(), d.clone()));
                                }
                            }
                            ui.label(RichText::new(format!("{}  Mapeado", icon::CHECK)).color(p.success));
                        } else {
                            ui.add_enabled_ui(res.dst.is_some(), |ui| {
                                if ui.button(format!("{}  Confirmar", icon::CHECK)).clicked() {
                                    res.confirmed = true;
                                    confirmed.push((name.clone(), res.dst.clone().unwrap_or_default()));
                                }
                            });
                        }
                        ui.end_row();
                    }
                });
            });
        });
    if !confirmed.is_empty() {
        let pair = app.store.pair_mut(&src, &dst);
        for (s, d) in confirmed {
            pair.set_activity(&s, &d);
        }
        app.save();
    }
    ui.add_space(8.0);
}

fn pending_projects(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui);
    let (Some(src), Some(dst)) = (app.sync.src.clone(), app.sync.dst.clone()) else { return };
    let Some(dst_projects) = app.sessions.get(&dst).map(|s| s.projects.clone()) else { return };
    let known = app.store.pair(&src, &dst).projects;
    let Some(plan) = app.sync.plan.as_mut() else { return };

    let suggestions = plan.project_suggestions(&known, &dst_projects);
    // Las imputaciones sin issue parten de la sugerencia deducida de los issues enlazados.
    for s in &suggestions {
        if let Some(res) = plan.projects.get_mut(&s.src_id)
            && !res.confirmed
            && res.dst.is_none()
        {
            res.dst = Some((s.dst_id, s.dst_name.clone()));
        }
    }
    let extra: Vec<&ProjectSuggestion> = suggestions
        .iter()
        .filter(|s| !plan.projects.contains_key(&s.src_id) && !app.sync.dismissed_projects.contains(&s.src_id))
        .collect();
    let pending_rows = plan.projects.values().any(|r| !r.confirmed);
    if !pending_rows && extra.is_empty() {
        return;
    }

    let mut confirmed: Vec<(u32, String, u32, String)> = Vec::new();
    let mut dismissed = Vec::new();
    theme::section_title(ui, icon::FOLDERS, "Proyectos de destino");
    theme::card(ui, |ui| {
        if pending_rows {
            ui.label(weak(ui, "Hay imputaciones sin issue. Elige en qué proyecto del destino se registrarán."));
            ui.add_space(6.0);
            egui::Grid::new("proj_grid").num_columns(4).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
                for (src_id, res) in plan.projects.iter_mut().filter(|(_, r)| !r.confirmed) {
                    ui.label(RichText::new(&res.src_name).family(theme::semibold()));
                    ui.label(RichText::new(icon::ARROW_RIGHT).color(p.text_weak));
                    egui::ComboBox::from_id_salt(("proj", *src_id))
                        .selected_text(res.dst.as_ref().map(|d| d.1.clone()).unwrap_or_else(|| "Elegir…".into()))
                        .width(280.0)
                        .show_ui(ui, |ui| {
                            for dp in &dst_projects {
                                ui.selectable_value(&mut res.dst, Some((dp.id, dp.name.clone())), &dp.name);
                            }
                        });
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(res.dst.is_some(), |ui| {
                            if ui.button(format!("{}  Confirmar", icon::CHECK)).clicked()
                                && let Some((id, name)) = &res.dst
                            {
                                confirmed.push((*src_id, res.src_name.clone(), *id, name.clone()));
                            }
                        });
                        let hint = suggestions
                            .iter()
                            .find(|s| s.src_id == *src_id && res.dst.as_ref().map(|d| d.0) == Some(s.dst_id));
                        if let Some(s) = hint {
                            theme::badge(ui, format!("sugerido · {} de {} issues", s.agreeing, s.linked), p.info)
                                .on_hover_text("Es el proyecto donde están la mayoría de sus issues enlazados");
                        }
                    });
                    ui.end_row();
                }
            });
        }
        if !extra.is_empty() {
            if pending_rows {
                ui.separator();
            }
            ui.label(weak(
                ui,
                "Según los issues enlazados, estos proyectos parecen corresponderse. Si lo confirmas, se \
                 usará para buscar issues en próximos análisis y como proyecto por defecto al crearlos.",
            ));
            ui.add_space(6.0);
            egui::Grid::new("proj_suggest_grid").num_columns(5).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
                for s in &extra {
                    ui.label(RichText::new(&s.src_name).family(theme::semibold()));
                    ui.label(RichText::new(icon::ARROW_RIGHT).color(p.text_weak));
                    ui.label(&s.dst_name);
                    theme::badge(ui, format!("{} de {} issues", s.agreeing, s.linked), p.info);
                    ui.horizontal(|ui| {
                        if ui.button(format!("{}  Confirmar", icon::CHECK)).clicked() {
                            confirmed.push((s.src_id, s.src_name.clone(), s.dst_id, s.dst_name.clone()));
                        }
                        if ui.button(RichText::new("Ignorar").color(p.text_weak)).clicked() {
                            dismissed.push(s.src_id);
                        }
                    });
                    ui.end_row();
                }
            });
        }
    });
    app.sync.dismissed_projects.extend(dismissed);
    if !confirmed.is_empty() {
        for (sid, _, did, dname) in &confirmed {
            plan.set_project(*sid, *did, dname);
        }
        let pair = app.store.pair_mut(&src, &dst);
        for (sid, sname, did, dname) in confirmed {
            pair.projects.insert(sid, ProjectLink { src_name: sname, dst_id: did, dst_name: dname });
        }
        app.save();
    }
    ui.add_space(8.0);
}

enum IssueAction {
    Confirm(u32, usize),
    Manual(u32, u32),
    Create(u32),
    Skip(u32),
}

fn pending_issues(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui);
    let Some(plan) = app.sync.plan.as_mut() else { return };
    let src_base = plan.src_base.clone();
    let pending: Vec<u32> = plan
        .issues
        .iter()
        .filter(|(_, r)| matches!(r.state, IssueState::Pending(_)))
        .map(|(id, _)| *id)
        .collect();
    if pending.is_empty() {
        return;
    }
    let hours: HashMap<u32, f64> = plan.rows.iter().fold(HashMap::new(), |mut acc, r| {
        if let Some(i) = r.entry.issue_id() {
            *acc.entry(i).or_default() += r.entry.hours;
        }
        acc
    });
    let mut action = None;
    ui.horizontal(|ui| {
        theme::section_title(ui, icon::QUESTION, "Issues por confirmar");
        theme::badge(ui, pending.len().to_string(), p.warning);
    });
    for id in pending {
        let res = plan.issues.get_mut(&id).expect("issue pendiente");
        let conf = match res.state {
            IssueState::Pending(c) => c,
            _ => Confidence::NotFound,
        };
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                theme::badge(ui, format!("#{}", id), p.accent);
                ui.hyperlink_to(RichText::new(&res.src.subject).family(theme::semibold()), format!("{src_base}/issues/{id}"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    match conf {
                        Confidence::NotFound => theme::badge(ui, "Sin equivalente claro", p.danger),
                        _ => theme::badge(ui, "Dudoso", p.warning),
                    };
                    ui.label(weak(ui, format!("{} · {}", res.src.project.name, theme::hours(*hours.get(&id).unwrap_or(&0.0)))));
                });
            });
            ui.add_space(4.0);
            let shown: Vec<_> = res.candidates.iter().enumerate().filter(|(_, c)| c.score >= 0.25).take(5).collect();
            if shown.is_empty() {
                ui.label(weak(ui, "No se ha encontrado ningún issue parecido en el destino."));
            }
            for (i, c) in shown {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut res.choice, i, "");
                    theme::confidence_badge(ui, c.score);
                    ui.label(weak(ui, format!("#{}", c.issue_id)));
                    ui.add(egui::Label::new(&c.subject).truncate()).on_hover_text(&c.reason);
                    ui.label(weak(ui, format!("· {} · {}", c.project, c.status)));
                });
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let has_choice = res.candidates.get(res.choice).is_some_and(|c| c.score >= 0.25);
                ui.add_enabled_ui(has_choice, |ui| {
                    if theme::primary_button(ui, format!("{}  Usar seleccionado", icon::CHECK)).clicked() {
                        action = Some(IssueAction::Confirm(id, res.choice));
                    }
                });
                if ui.button(format!("{}  Crear issue en destino", icon::FILE_PLUS)).clicked() {
                    action = Some(IssueAction::Create(id));
                }
                ui.add_space(8.0);
                ui.add(TextEdit::singleline(&mut res.manual).hint_text("Nº de issue").desired_width(110.0));
                let manual = res.manual.trim().trim_start_matches('#').parse::<u32>().ok();
                ui.add_enabled_ui(manual.is_some(), |ui| {
                    if ui.button(format!("{}  Enlazar", icon::LINK)).clicked() {
                        action = manual.map(|m| IssueAction::Manual(id, m));
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(RichText::new(format!("{}  Omitir", icon::PROHIBIT)).color(p.text_weak)).clicked() {
                        action = Some(IssueAction::Skip(id));
                    }
                });
            });
        });
        ui.add_space(6.0);
    }
    match action {
        Some(IssueAction::Confirm(id, idx)) => {
            let Some(c) = plan.issues.get(&id).and_then(|r| r.candidates.get(idx)).cloned() else { return };
            let issue = plan.data.dst_pool.iter().find(|i| i.id == c.issue_id).cloned();
            if let Some(issue) = issue {
                confirm_link(app, id, &issue, LinkOrigin::Confirmed);
            }
        }
        Some(IssueAction::Manual(id, dst_id)) => {
            if let Some(issue) = plan.data.dst_pool.iter().find(|i| i.id == dst_id).cloned() {
                confirm_link(app, id, &issue, LinkOrigin::Confirmed);
            } else if let Some(client) = app.sync.dst.as_ref().and_then(|d| app.sessions.get(d)).map(|s| s.client.clone()) {
                app.worker.spawn(move || Msg::ManualIssue {
                    src_issue: id,
                    result: client.issue(dst_id).map_err(|e| e.to_string()),
                });
            }
        }
        Some(IssueAction::Create(id)) => open_create(app, id),
        Some(IssueAction::Skip(id)) => {
            if let Some(r) = plan.issues.get_mut(&id) {
                r.state = IssueState::Skipped;
            }
            for row in plan.rows.iter_mut().filter(|r| r.entry.issue_id() == Some(id)) {
                row.include = false;
            }
        }
        None => {}
    }
    ui.add_space(8.0);
}

fn rows_table(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui);
    let Some(plan) = app.sync.plan.as_mut() else { return };
    let busy = app.sync.exec.as_ref().is_some_and(|e| !e.finished);
    ui.horizontal(|ui| {
        theme::section_title(ui, icon::LIST_CHECKS, "Imputaciones");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.toggle_value(&mut app.sync.only_pending, "Ocultar ya sincronizadas");
        });
    });
    if plan.rows.is_empty() {
        theme::empty_state(ui, icon::CLOCK, "Sin imputaciones", "No hay imputaciones tuyas en el origen para este periodo.");
        return;
    }
    let visible: Vec<usize> = (0..plan.rows.len())
        .filter(|&i| !app.sync.only_pending || !matches!(plan.rows[i].sync, RowSync::Already { .. } | RowSync::Done { .. }))
        .collect();
    let mut unlink = None;
    let mut toggles = Vec::new();
    let dst_base = plan.dst_base.clone();
    theme::card_frame(ui).inner_margin(Margin::same(8)).show(ui, |ui| {
        TableBuilder::new(ui)
            .id_salt("plan_rows")
            .striped(true)
            .vscroll(false)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(28.0))
            .column(Column::exact(84.0))
            .column(Column::exact(56.0))
            .column(Column::initial(230.0).at_least(140.0).clip(true).resizable(true))
            .column(Column::initial(250.0).at_least(140.0).clip(true).resizable(true))
            .column(Column::initial(190.0).at_least(120.0).clip(true).resizable(true))
            .column(Column::remainder().at_least(150.0).clip(true))
            .header(26.0, |mut h| {
                for t in ["", "Fecha", "Horas", "Issue origen", "Issue destino", "Actividad", "Estado"] {
                    h.col(|ui| {
                        ui.label(weak(ui, t).size(12.0));
                    });
                }
            })
            .body(|body| {
                body.rows(34.0, visible.len(), |mut row| {
                    let idx = visible[row.index()];
                    let r = &plan.rows[idx];
                    let readiness = plan.readiness(r);
                    let finished = matches!(readiness, Readiness::Finished);
                    row.col(|ui| {
                        let mut inc = r.include;
                        if ui.add_enabled(!finished && !busy, egui::Checkbox::without_text(&mut inc)).changed() {
                            toggles.push((idx, inc));
                        }
                    });
                    row.col(|ui| {
                        ui.label(dates::pretty(&r.entry.spent_on)).on_hover_text(dates::weekday_es(
                            dates::parse(&r.entry.spent_on).unwrap_or_else(dates::today),
                        ));
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(theme::hours(r.entry.hours)).family(theme::semibold()));
                    });
                    row.col(|ui| match r.entry.issue_id().and_then(|i| plan.issues.get(&i)) {
                        Some(res) => {
                            ui.add(egui::Label::new(format!("#{} {}", res.src.id, res.src.subject)).truncate())
                                .on_hover_text(format!("{}\n\n{}", res.src.subject, r.entry.comments_text()));
                        }
                        None => {
                            ui.label(weak(ui, format!("{}  {}", icon::FOLDER, r.entry.project.name)));
                        }
                    });
                    row.col(|ui| match r.entry.issue_id().and_then(|i| plan.issues.get(&i).map(|res| (i, res))) {
                        Some((sid, res)) => match &res.state {
                            IssueState::Linked { dst_id, subject, origin, score, project, .. } => {
                                if !finished && !busy && theme::icon_button(ui, icon::LINK_BREAK, "Cambiar el issue de destino").clicked() {
                                    unlink = Some(sid);
                                }
                                if *origin == LinkOrigin::Auto {
                                    theme::confidence_badge(ui, *score);
                                }
                                ui.hyperlink_to(format!("#{dst_id}"), format!("{dst_base}/issues/{dst_id}"));
                                ui.add(egui::Label::new(subject).truncate())
                                    .on_hover_text(format!("{subject}
Proyecto: {project}"));
                            }
                            IssueState::Pending(_) => {
                                ui.label(RichText::new(format!("{}  Por confirmar", icon::QUESTION)).color(p.warning));
                            }
                            IssueState::Skipped => {
                                ui.label(weak(ui, "Omitido"));
                            }
                        },
                        None => match plan.projects.get(&r.entry.project.id) {
                            Some(pr) if pr.confirmed => {
                                ui.label(format!("{}  {}", icon::FOLDER, pr.dst.as_ref().map(|d| d.1.as_str()).unwrap_or("")));
                            }
                            _ => {
                                ui.label(RichText::new("Proyecto por elegir").color(p.warning));
                            }
                        },
                    });
                    row.col(|ui| {
                        let src = &r.entry.activity.name;
                        match plan.activity_for_row(r) {
                            Some(dst) if matching::same_name(src, &dst) => {
                                theme::badge(ui, dst, p.info);
                            }
                            Some(dst) => {
                                ui.label(weak(ui, src));
                                ui.label(weak(ui, icon::ARROW_RIGHT));
                                theme::badge(ui, dst, p.info);
                            }
                            None => {
                                ui.label(weak(ui, src));
                                ui.label(RichText::new(icon::QUESTION).color(p.warning));
                            }
                        }
                    });
                    row.col(|ui| row_status(ui, &p, r.include, &r.sync, &readiness));
                });
            });
    });
    for (idx, inc) in toggles {
        if let Some(r) = plan.rows.get_mut(idx) {
            r.include = inc;
        }
    }
    if let Some(sid) = unlink {
        plan.unlink_issue(sid);
    }
}

fn row_status(ui: &mut Ui, p: &theme::Palette, include: bool, sync: &RowSync, readiness: &Readiness) {
    match sync {
        RowSync::Done { dst_entry_id } => {
            theme::badge(ui, format!("{}  Creada #{dst_entry_id}", icon::CHECK), p.success);
        }
        RowSync::Already { dst_entry_id } => {
            theme::badge(ui, format!("{}  Ya sincronizada", icon::LINK), p.info)
                .on_hover_text(format!("Imputación #{dst_entry_id} en el destino"));
        }
        RowSync::Failed(e) => {
            theme::badge(ui, format!("{}  Error", icon::WARNING), p.danger).on_hover_text(e);
            ui.add(egui::Label::new(RichText::new(e).color(p.danger).size(12.0)).truncate());
        }
        RowSync::Duplicate { dst_entry_id } => {
            theme::badge(ui, format!("{}  ¿Duplicada?", icon::COPY), p.warning).on_hover_text(format!(
                "Ya existe la imputación #{dst_entry_id} en el destino con la misma fecha, horas e issue. Márcala solo si quieres crearla igualmente."
            ));
        }
        RowSync::New => match readiness {
            _ if !include => {
                ui.label(RichText::new("No se enviará").color(p.text_weak));
            }
            Readiness::Ready { .. } => {
                theme::badge(ui, format!("{}  Lista", icon::CHECK_CIRCLE), p.success);
            }
            Readiness::Blocked(why) => {
                theme::badge(ui, format!("{}  {why}", icon::QUESTION), p.warning);
            }
            Readiness::Finished => {}
        },
    }
}

fn footer(app: &mut App, ui: &mut Ui, diffs: &[IssueDiff]) {
    let p = theme::palette(ui);
    let Some(plan) = &app.sync.plan else { return };
    let items = plan.build_items(app.store.settings.add_marker);
    let hours: f64 = items.iter().map(|i| i.new.hours).sum();
    let updates = diffs
        .iter()
        .filter(|d| !d.changes.is_empty() && !app.sync.issue_skip.contains(&d.src_id))
        .count();
    let mut go = false;
    let mut settings_changed = false;
    theme::card(ui, |ui| {
        if let Some(exec) = &app.sync.exec {
            let frac = exec.done as f32 / exec.total.max(1) as f32;
            ui.add(
                egui::ProgressBar::new(frac)
                    .text(format!(
                        "{}/{} · {} imputaciones · {} issues actualizados · {} con error",
                        exec.done, exec.total, exec.ok, exec.issues_ok, exec.failed
                    ))
                    .fill(if exec.failed > 0 || !exec.issue_warnings.is_empty() { p.warning } else { p.success }),
            );
            ui.add_space(6.0);
        }
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                settings_changed |= ui
                    .checkbox(
                        &mut app.store.settings.update_issues,
                        "Igualar también los issues de destino con los de origen",
                    )
                    .on_hover_text("Estado, % realizado, fechas, estimación y versión (configurable en Ajustes). El issue de origen nunca se modifica.")
                    .changed();
                let marker = if app.store.settings.add_marker {
                    format!("Se añadirá la marca [sync:{}#…] al comentario en el destino.", app.sync.src.as_deref().unwrap_or(""))
                } else {
                    "Sin marca de sincronización en los comentarios (desactivada en Ajustes).".into()
                };
                ui.label(weak(ui, marker));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let busy = app.sync.exec.as_ref().is_some_and(|e| !e.finished);
                let nothing = items.is_empty() && updates == 0;
                ui.add_enabled_ui(!nothing && !busy, |ui| {
                    let mut parts = Vec::new();
                    if !items.is_empty() {
                        parts.push(format!("crear {} ({})", plural(items.len(), "imputación", "imputaciones"), theme::hours(hours)));
                    }
                    if updates > 0 {
                        parts.push(format!("actualizar {}", plural(updates, "issue", "issues")));
                    }
                    let text = if nothing {
                        "Nada que sincronizar".to_string()
                    } else {
                        let joined = parts.join(" y ");
                        let mut chars = joined.chars();
                        let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
                        format!("{}  {first}{}", icon::ROCKET_LAUNCH, chars.as_str())
                    };
                    if theme::primary_button(ui, text).clicked() {
                        go = true;
                    }
                });
            });
        });
    });
    if settings_changed {
        app.save();
    }
    if go {
        execute(app, diffs);
    }
}

// ---- Diálogo de creación de issue ---------------------------------------------

pub struct CreateDialog {
    src_issue: u32,
    src: Issue,
    src_url: String,
    project: Option<u32>,
    trackers: Loadable<Vec<Tracker>>,
    tracker: Option<u32>,
    priorities: Vec<Priority>,
    priority: Option<u32>,
    subject: String,
    description: String,
    start_date: String,
    due_date: String,
    estimated: String,
    done_ratio: u32,
    assign_to_me: bool,
    add_reference: bool,
    fields: Loadable<Vec<(CustomFieldValue, String)>>,
    submitting: bool,
    error: Option<String>,
}

fn open_create(app: &mut App, src_issue: u32) {
    let (Some(src), Some(dst)) = (app.sync.src.clone(), app.sync.dst.clone()) else { return };
    let Some(plan) = &app.sync.plan else { return };
    let Some(res) = plan.issues.get(&src_issue) else { return };
    let mappings = app.store.pair(&src, &dst);
    let dst_projects = app.sessions.get(&dst).map(|s| s.projects.clone()).unwrap_or_default();
    // Proyecto: mapeo guardado, el del mejor candidato o el de nombre más parecido.
    let project = mappings
        .projects
        .get(&res.src.project.id)
        .map(|l| l.dst_id)
        .or_else(|| res.candidates.first().filter(|c| c.score >= 0.4).map(|c| c.project_id))
        .or_else(|| {
            let names: Vec<String> = dst_projects.iter().map(|p| p.name.clone()).collect();
            matching::suggest_name(&res.src.project.name, &names).filter(|(_, s)| *s >= 0.5).map(|(i, _)| dst_projects[i].id)
        })
        .filter(|id| dst_projects.iter().any(|p| p.id == *id));
    let src_url = format!("{}/issues/{}", plan.src_base, src_issue);
    let s = &res.src;
    app.sync.create = Some(CreateDialog {
        src_issue,
        src: s.clone(),
        src_url,
        project,
        trackers: Loadable::Idle,
        tracker: None,
        priorities: vec![],
        priority: None,
        subject: s.subject.clone(),
        description: s.description_text().to_string(),
        start_date: s.start_date.clone().unwrap_or_default(),
        due_date: s.due_date.clone().unwrap_or_default(),
        estimated: s.estimated_hours.map(|h| h.to_string()).unwrap_or_default(),
        done_ratio: s.done_ratio.unwrap_or(0.0) as u32,
        assign_to_me: true,
        add_reference: true,
        fields: Loadable::Idle,
        submitting: false,
        error: None,
    });
}

fn create_dialog(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui);
    let Some(dst) = app.sync.dst.clone() else { return };
    let Some(session) = app.sessions.get(&dst) else { return };
    let client = session.client.clone();
    let user_id = session.user.id;
    let projects = session.projects.clone();
    let dst_name = app.store.instance_name(&dst);
    let Some(dlg) = app.sync.create.as_mut() else { return };

    // Carga perezosa de tipos/prioridades y de campos personalizados.
    if let Some(project) = dlg.project {
        if matches!(dlg.trackers, Loadable::Idle) {
            dlg.trackers = Loadable::Loading;
            let c = client.clone();
            app.worker.spawn(move || Msg::CreateMeta {
                project,
                result: (|| Ok((c.project_trackers(project)?, c.priorities()?)))()
                    .map_err(|e: crate::redmine::ApiError| e.to_string()),
            });
        }
        if let (Some(tracker), Loadable::Idle) = (dlg.tracker, &dlg.fields) {
            dlg.fields = Loadable::Loading;
            let c = client.clone();
            app.worker.spawn(move || Msg::CustomFields {
                project,
                tracker,
                result: c.sample_custom_fields(project, tracker).map_err(|e| e.to_string()),
            });
        }
    }

    let mut close = false;
    let mut submit = false;
    let modal = egui::Modal::new(egui::Id::unique("create_issue")).show(ui.ctx(), |ui| {
        ui.set_width(640.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon::FILE_PLUS).size(22.0).color(p.accent));
            ui.label(RichText::new(format!("Crear issue en {dst_name}")).size(19.0).family(theme::semibold()));
        });
        ui.label(weak(ui, format!("A partir de #{} · {}", dlg.src.id, dlg.src.project.name)));
        ui.add_space(8.0);
        egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
            egui::Grid::new("create_grid").num_columns(2).spacing(vec2(12.0, 10.0)).show(ui, |ui| {
                ui.label("Proyecto");
                let before = dlg.project;
                egui::ComboBox::from_id_salt("cr_project")
                    .selected_text(
                        dlg.project
                            .and_then(|id| projects.iter().find(|p| p.id == id))
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| "Elegir proyecto…".into()),
                    )
                    .width(420.0)
                    .show_ui(ui, |ui| {
                        for pr in &projects {
                            ui.selectable_value(&mut dlg.project, Some(pr.id), &pr.name);
                        }
                    });
                if dlg.project != before {
                    dlg.trackers = Loadable::Idle;
                    dlg.fields = Loadable::Idle;
                    dlg.tracker = None;
                }
                ui.end_row();

                ui.label("Tipo");
                match &dlg.trackers {
                    Loadable::Ready(trackers) => { ui.horizontal(|ui| {
                        let before = dlg.tracker;
                        egui::ComboBox::from_id_salt("cr_tracker")
                            .selected_text(trackers.iter().find(|t| Some(t.id) == dlg.tracker).map(|t| t.name.clone()).unwrap_or_default())
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                for t in trackers {
                                    ui.selectable_value(&mut dlg.tracker, Some(t.id), &t.name);
                                }
                            });
                        if dlg.tracker != before {
                            dlg.fields = Loadable::Idle;
                        }
                        ui.label(weak(ui, format!("origen: {}", dlg.src.tracker.name)));
                    }); }
                    Loadable::Failed(e) => {
                        ui.label(RichText::new(e).color(p.danger));
                    }
                    _ if dlg.project.is_none() => {
                        ui.label(weak(ui, "Elige primero un proyecto"));
                    }
                    _ => {
                        ui.spinner();
                    }
                }
                ui.end_row();

                ui.label("Asunto");
                ui.add(TextEdit::singleline(&mut dlg.subject).desired_width(460.0));
                ui.end_row();

                ui.label("Descripción");
                ui.add(TextEdit::multiline(&mut dlg.description).desired_rows(5).desired_width(460.0));
                ui.end_row();

                ui.label("Prioridad");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("cr_priority")
                        .selected_text(dlg.priorities.iter().find(|p| Some(p.id) == dlg.priority).map(|p| p.name.clone()).unwrap_or_else(|| "Por defecto".into()))
                        .width(180.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut dlg.priority, None, "Por defecto");
                            for pr in &dlg.priorities {
                                ui.selectable_value(&mut dlg.priority, Some(pr.id), &pr.name);
                            }
                        });
                    ui.label(weak(ui, format!("origen: {}", dlg.src.priority.name)));
                });
                ui.end_row();

                ui.label("Fechas");
                ui.horizontal(|ui| {
                    ui.add(TextEdit::singleline(&mut dlg.start_date).hint_text("inicio AAAA-MM-DD").desired_width(150.0));
                    ui.label("→");
                    ui.add(TextEdit::singleline(&mut dlg.due_date).hint_text("fin AAAA-MM-DD").desired_width(150.0));
                });
                ui.end_row();

                ui.label("Estimación");
                ui.horizontal(|ui| {
                    ui.add(TextEdit::singleline(&mut dlg.estimated).hint_text("horas").desired_width(70.0));
                    ui.label(weak(ui, "Realizado"));
                    ui.add(egui::Slider::new(&mut dlg.done_ratio, 0..=100).step_by(10.0).suffix(" %"));
                });
                ui.end_row();

                if let Loadable::Ready(fields) = &mut dlg.fields {
                    for (f, value) in fields.iter_mut() {
                        ui.label(&f.name);
                        ui.add(TextEdit::singleline(value).desired_width(300.0).hint_text(if f.multiple { "valores separados por comas" } else { "" }));
                        ui.end_row();
                    }
                }
                if dlg.fields.is_loading() {
                    ui.label("");
                    ui.label(weak(ui, "Buscando campos personalizados del proyecto…"));
                    ui.end_row();
                }

                ui.label("");
                ui.vertical(|ui| {
                    ui.checkbox(&mut dlg.assign_to_me, "Asignármelo");
                    ui.checkbox(&mut dlg.add_reference, "Añadir enlace al issue de origen en la descripción")
                        .on_hover_text("Permite reconocer el emparejamiento automáticamente en el futuro.");
                });
                ui.end_row();
            });
        });
        if let Some(e) = &dlg.error {
            ui.add_space(6.0);
            theme::callout(ui, icon::WARNING_OCTAGON, p.danger, e.clone());
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let ready = dlg.project.is_some() && dlg.tracker.is_some() && !dlg.subject.trim().is_empty() && !dlg.submitting;
                ui.add_enabled_ui(ready, |ui| {
                    if theme::primary_button(ui, format!("{}  Crear issue", icon::CHECK)).clicked() {
                        submit = true;
                    }
                });
                if ui.button("Cancelar").clicked() {
                    close = true;
                }
                if dlg.submitting {
                    ui.spinner();
                }
            });
        });
    });
    if modal.should_close() && !dlg.submitting {
        close = true;
    }

    if submit {
        let mut description = dlg.description.trim().to_string();
        if dlg.add_reference {
            description = format!("{description}\n\n---\nOrigen: {}", dlg.src_url).trim().to_string();
        }
        let custom_fields = match &dlg.fields {
            Loadable::Ready(fields) => fields
                .iter()
                .filter(|(_, v)| !v.trim().is_empty())
                .map(|(f, v)| CustomFieldInput {
                    id: f.id,
                    value: if f.multiple {
                        serde_json::Value::Array(v.split(',').map(|s| serde_json::Value::String(s.trim().to_string())).collect())
                    } else {
                        serde_json::Value::String(v.trim().to_string())
                    },
                })
                .collect(),
            _ => vec![],
        };
        let new = NewIssue {
            project_id: dlg.project.unwrap_or_default(),
            tracker_id: dlg.tracker.unwrap_or_default(),
            subject: dlg.subject.trim().to_string(),
            description,
            status_id: None,
            priority_id: dlg.priority,
            assigned_to_id: dlg.assign_to_me.then_some(user_id),
            start_date: dates::parse(&dlg.start_date).map(dates::iso),
            due_date: dates::parse(&dlg.due_date).map(dates::iso),
            estimated_hours: dlg.estimated.trim().replace(',', ".").parse().ok(),
            done_ratio: (dlg.done_ratio > 0).then_some(dlg.done_ratio),
            custom_fields,
        };
        dlg.submitting = true;
        dlg.error = None;
        let src_issue = dlg.src_issue;
        app.worker.spawn(move || Msg::IssueCreated {
            src_issue,
            result: client.create_issue(&new).map_err(|e| e.to_string()),
        });
    } else if close {
        app.sync.create = None;
    }
}

