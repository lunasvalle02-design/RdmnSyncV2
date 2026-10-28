//! Estado global de la aplicación, enrutado de pantallas y procesado de mensajes de fondo.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use egui::{Align, Layout, Margin, RichText, Stroke, Theme, Ui};

use crate::redmine::{Activity, Client, CustomFieldValue, Issue, Priority, Project, TimeEntry, Tracker, User};
use crate::store::Store;
use crate::sync::{ExecItem, SyncData};
use crate::theme::{self, icon};
use crate::ui::{explorer, login, mappings, settings, summary, sync_view};
use crate::worker::Worker;

/// Conexión activa con un Redmine.
pub struct Session {
    pub client: Client,
    pub user: User,
    pub projects: Vec<Project>,
    pub activities: Vec<Activity>,
}

pub enum Loadable<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Loadable<T> {
    pub fn is_loading(&self) -> bool {
        matches!(self, Loadable::Loading)
    }
}

pub enum Msg {
    Connected { inst: String, key: String, result: Result<Box<Session>, String> },
    ExplorerIssues { seq: u64, result: Result<Vec<Issue>, String> },
    ExplorerEntries { seq: u64, result: Result<Vec<TimeEntry>, String> },
    PlanLoaded { seq: u64, result: Result<SyncData, String> },
    ManualIssue { src_issue: u32, result: Result<Issue, String> },
    CreateMeta { project: u32, result: Result<(Vec<Tracker>, Vec<Priority>), String> },
    CustomFields { project: u32, tracker: u32, result: Result<Vec<CustomFieldValue>, String> },
    IssueCreated { src_issue: u32, result: Result<Issue, String> },
    Pushed { item: ExecItem, result: Result<TimeEntry, String> },
    ExecFinished,
    Undone { batch: String, ok: Vec<u32>, errors: Vec<String> },
    Weekly { seq: u64, results: Vec<(String, Result<Vec<TimeEntry>, String>)> },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Explorer,
    Sync,
    Mappings,
    Summary,
    Settings,
}

#[derive(Clone, Copy)]
pub enum ToastKind {
    Success,
    Warning,
    Error,
}

struct Toast {
    kind: ToastKind,
    text: String,
    until: Instant,
}

pub struct App {
    pub store: Store,
    pub worker: Worker<Msg>,
    pub sessions: BTreeMap<String, Session>,
    pub show_login: bool,
    pub tab: Tab,
    pub login: login::LoginState,
    pub explorer: explorer::ExplorerState,
    pub sync: sync_view::SyncState,
    pub mappings: mappings::MappingsState,
    pub summary: summary::SummaryState,
    toasts: Vec<Toast>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let store = Store::load();
        cc.egui_ctx.set_theme(if store.settings.dark_mode { Theme::Dark } else { Theme::Light });
        let mut app = Self {
            login: login::LoginState::new(&store),
            store,
            worker: Worker::new(cc.egui_ctx.clone()),
            sessions: BTreeMap::new(),
            show_login: true,
            tab: Tab::Explorer,
            explorer: Default::default(),
            sync: Default::default(),
            mappings: Default::default(),
            summary: Default::default(),
            toasts: Vec::new(),
        };
        if let Some(e) = app.store.last_error.take() {
            app.toast(ToastKind::Error, e);
        }
        login::autoconnect(&mut app);
        app
    }

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        let secs = match kind {
            ToastKind::Error => 8,
            ToastKind::Warning => 6,
            _ => 4,
        };
        self.toasts.push(Toast { kind, text: text.into(), until: Instant::now() + Duration::from_secs(secs) });
    }

    pub fn save(&mut self) {
        self.store.save();
        if let Some(e) = self.store.last_error.take() {
            self.toast(ToastKind::Error, e);
        }
    }

    /// Instancias conectadas, en el orden en que están configuradas.
    pub fn connected_ids(&self) -> Vec<String> {
        self.store
            .instances
            .iter()
            .filter(|i| self.sessions.contains_key(&i.id))
            .map(|i| i.id.clone())
            .collect()
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Connected { inst, key, result } => login::on_connected(self, inst, key, result),
            Msg::ExplorerIssues { seq, result } => explorer::on_issues(self, seq, result),
            Msg::ExplorerEntries { seq, result } => explorer::on_entries(self, seq, result),
            Msg::PlanLoaded { seq, result } => sync_view::on_plan(self, seq, result),
            Msg::ManualIssue { src_issue, result } => sync_view::on_manual_issue(self, src_issue, result),
            Msg::CreateMeta { project, result } => sync_view::on_create_meta(self, project, result),
            Msg::CustomFields { project, tracker, result } => {
                sync_view::on_custom_fields(self, project, tracker, result)
            }
            Msg::IssueCreated { src_issue, result } => sync_view::on_issue_created(self, src_issue, result),
            Msg::Pushed { item, result } => sync_view::on_pushed(self, item, result),
            Msg::ExecFinished => sync_view::on_exec_finished(self),
            Msg::Undone { batch, ok, errors } => mappings::on_undone(self, batch, ok, errors),
            Msg::Weekly { seq, results } => summary::on_weekly(self, seq, results),
        }
    }

    fn top_bar(&mut self, ui: &mut Ui) {
        let p = theme::palette(ui);
        ui.horizontal(|ui| {
            theme::logo(ui, 30.0);
            ui.label(RichText::new("RdmnSync").size(18.0).family(theme::semibold()));
            ui.add_space(18.0);

            let tabs = [
                (Tab::Explorer, icon::FOLDERS, "Explorador"),
                (Tab::Sync, icon::ARROWS_CLOCKWISE, "Sincronizar"),
                (Tab::Mappings, icon::GIT_DIFF, "Mapeos"),
                (Tab::Summary, icon::CALENDAR_DOTS, "Resumen semanal"),
                (Tab::Settings, icon::GEAR_SIX, "Ajustes"),
            ];
            for (tab, ic, label) in tabs {
                if theme::pill(ui, self.tab == tab, format!("{ic}  {label}")).clicked() {
                    self.tab = tab;
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let dark = ui.visuals().dark_mode;
                let ic = if dark { icon::SUN } else { icon::MOON };
                if theme::icon_button(ui, ic, "Cambiar tema").clicked() {
                    self.store.settings.dark_mode = !dark;
                    ui.ctx().set_theme(if dark { Theme::Light } else { Theme::Dark });
                    self.save();
                }
                if theme::icon_button(ui, icon::PLUGS, "Gestionar conexiones").clicked() {
                    self.show_login = true;
                }
                for id in self.connected_ids().iter().rev() {
                    let user = self.sessions[id].user.display_name();
                    egui::Frame::new()
                        .fill(p.control)
                        .corner_radius(99)
                        .inner_margin(Margin::symmetric(10, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                theme::status_dot(ui, p.success);
                                ui.label(RichText::new(self.store.instance_name(id)).size(12.5));
                            });
                        })
                        .response
                        .on_hover_text(format!("Conectado como {user}"));
                }
                if self.worker.busy() {
                    ui.spinner();
                }
            });
        });
    }

    fn show_toasts(&mut self, ui: &mut Ui) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        let p = theme::palette(ui);
        let ctx = ui.ctx().clone();
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-18.0, -18.0])
            .order(egui::Order::Foreground)
            .show(&ctx, |ui| {
                ui.set_max_width(420.0);
                let mut close = None;
                for (i, t) in self.toasts.iter().enumerate() {
                    let (ic, color) = match t.kind {
                        ToastKind::Success => (icon::CHECK_CIRCLE, p.success),
                        ToastKind::Warning => (icon::WARNING, p.warning),
                        ToastKind::Error => (icon::WARNING_OCTAGON, p.danger),
                    };
                    egui::Frame::new()
                        .fill(p.card)
                        .stroke(Stroke::new(1.0, color.gamma_multiply(0.5)))
                        .corner_radius(10)
                        .inner_margin(Margin::symmetric(14, 10))
                        .shadow(ui.visuals().popup_shadow)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(ic).size(18.0).color(color));
                                ui.add(egui::Label::new(&t.text).wrap());
                                if ui.small_button(icon::X).clicked() {
                                    close = Some(i);
                                }
                            });
                        });
                    ui.add_space(4.0);
                }
                if let Some(i) = close {
                    self.toasts.remove(i);
                }
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        for msg in self.worker.poll() {
            self.handle(msg);
        }
        #[cfg(debug_assertions)]
        self.dev_autopilot();
        let p = theme::palette(ui);

        if self.show_login || self.sessions.is_empty() {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin::same(24)))
                .show(ui, |ui| login::show(self, ui));
        } else {
            egui::Panel::top("top_bar")
                .frame(
                    egui::Frame::new()
                        .fill(p.surface)
                        .inner_margin(Margin::symmetric(16, 10))
                        .stroke(Stroke::new(1.0, p.border)),
                )
                .show(ui, |ui| self.top_bar(ui));
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin::same(0)))
                .show(ui, |ui| match self.tab {
                    Tab::Explorer => explorer::show(self, ui),
                    Tab::Sync => sync_view::show(self, ui),
                    Tab::Mappings => mappings::show(self, ui),
                    Tab::Summary => summary::show(self, ui),
                    Tab::Settings => settings::show(self, ui),
                });
        }
        self.show_toasts(ui);
    }
}

#[cfg(debug_assertions)]
impl App {
    /// Solo en desarrollo: `RDMNSYNC_DEV_TAB=explorer|sync|mappings|summary|settings` salta el login
    /// cuando hay al menos dos conexiones y lanza la acción principal de la pestaña (para capturas).
    fn dev_autopilot(&mut self) {
        let Ok(tab) = std::env::var("RDMNSYNC_DEV_TAB") else { return };
        if self.sessions.len() < 2 || !self.show_login {
            return;
        }
        self.show_login = false;
        self.tab = match tab.as_str() {
            "sync" => Tab::Sync,
            "mappings" => Tab::Mappings,
            "summary" => Tab::Summary,
            "settings" => Tab::Settings,
            _ => Tab::Explorer,
        };
        match self.tab {
            Tab::Explorer => explorer::dev_open_first(self),
            Tab::Sync => sync_view::dev_analyze(self),
            _ => {}
        }
    }
}

/// Contenedor con margen y scroll para las páginas de contenido.
pub fn page(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new().inner_margin(Margin::symmetric(24, 18)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                add(ui)
            });
        });
}

pub fn weak(ui: &Ui, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(theme::palette(ui).text_weak)
}

pub mod dates {
    use chrono::{Datelike, Duration, Local, NaiveDate};

    pub fn today() -> NaiveDate {
        Local::now().date_naive()
    }

    pub fn week_of(d: NaiveDate) -> (NaiveDate, NaiveDate) {
        let monday = d - Duration::days(d.weekday().num_days_from_monday() as i64);
        (monday, monday + Duration::days(6))
    }

    pub fn month_of(d: NaiveDate) -> (NaiveDate, NaiveDate) {
        let first = d.with_day(1).unwrap_or(d);
        let next = if first.month() == 12 {
            NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
        };
        (first, next.map(|n| n - Duration::days(1)).unwrap_or(first))
    }

    pub fn parse(s: &str) -> Option<NaiveDate> {
        NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
    }

    pub fn iso(d: NaiveDate) -> String {
        d.format("%Y-%m-%d").to_string()
    }

    pub fn weekday_es(d: NaiveDate) -> &'static str {
        ["Lunes", "Martes", "Miércoles", "Jueves", "Viernes", "Sábado", "Domingo"]
            [d.weekday().num_days_from_monday() as usize]
    }

    /// "2026-10-28" → "28/10/2026"
    pub fn pretty(s: &str) -> String {
        parse(s).map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_else(|| s.to_string())
    }
}
