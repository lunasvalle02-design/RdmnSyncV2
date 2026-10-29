//! Estado global de la aplicación, enrutado de pantallas y procesado de mensajes de fondo.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use egui::emath::easing;
use egui::{Align, Layout, Margin, Rect, RichText, Stroke, Theme, Ui, pos2, vec2};

use rdmnsync_core::redmine::{
    Activity, Client, CustomFieldValue, Issue, IssueStatus, Priority, Project, TimeEntry, Tracker, User, Version,
};
use rdmnsync_core::store::Store;
use rdmnsync_core::sync::{ExecItem, SyncData};

use crate::theme::{self, icon};
use crate::ui::{explorer, login, mappings, settings, summary, sync_view};
use crate::worker::Worker;

/// Conexión activa con un Redmine.
pub struct Session {
    pub client: Client,
    pub user: User,
    pub projects: Vec<Project>,
    pub activities: Vec<Activity>,
    pub statuses: Vec<IssueStatus>,
    pub priorities: Vec<Priority>,
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
    Versions { inst: String, project: u32, result: Result<Vec<Version>, String> },
    IssueUpdated {
        src_issue: u32,
        snapshot: BTreeMap<String, String>,
        auto_versions: Vec<(u32, String, u32, String)>,
        result: Result<(Issue, Vec<String>), String>,
    },
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
    id: u64,
    kind: ToastKind,
    text: String,
    born: Instant,
    until: Instant,
    secs: f32,
}

/// Duración de la entrada y la salida de un aviso.
const TOAST_FADE: f32 = 0.25;

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
    /// Versiones por (instancia, proyecto), cargadas bajo demanda.
    pub versions: HashMap<(String, u32), Loadable<Vec<Version>>>,
    toasts: Vec<Toast>,
    toast_seq: u64,
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
            versions: HashMap::new(),
            toasts: Vec::new(),
            toast_seq: 0,
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
        let now = Instant::now();
        self.toast_seq += 1;
        self.toasts.push(Toast {
            id: self.toast_seq,
            kind,
            text: text.into(),
            born: now,
            until: now + Duration::from_secs(secs),
            secs: secs as f32,
        });
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

    pub fn request_versions(&mut self, inst: &str, project: u32) {
        let key = (inst.to_string(), project);
        if self.versions.contains_key(&key) {
            return;
        }
        let Some(client) = self.sessions.get(inst).map(|s| s.client.clone()) else { return };
        self.versions.insert(key, Loadable::Loading);
        let inst = inst.to_string();
        self.worker.spawn(move || Msg::Versions {
            inst,
            project,
            result: client.versions(project).map_err(|e| e.to_string()),
        });
    }

    /// Versiones ya cargadas de un proyecto (vacío si aún no están).
    pub fn versions_of(&self, inst: &str, project: u32) -> &[Version] {
        match self.versions.get(&(inst.to_string(), project)) {
            Some(Loadable::Ready(v)) => v,
            _ => &[],
        }
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
            Msg::Versions { inst, project, result } => {
                self.versions.insert((inst, project), result.map_or_else(Loadable::Failed, Loadable::Ready));
            }
            Msg::IssueUpdated { src_issue, snapshot, auto_versions, result } => {
                sync_view::on_issue_updated(self, src_issue, snapshot, auto_versions, result)
            }
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
            let labels: Vec<String> = tabs.iter().map(|(_, ic, label)| format!("{ic}  {label}")).collect();
            let current = tabs.iter().position(|(t, ..)| *t == self.tab);
            if let Some(i) = theme::segmented(ui, "top_tabs", current, &labels) {
                self.tab = tabs[i].0;
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
                                theme::status_dot(ui, p.success, false);
                                ui.label(RichText::new(self.store.instance_name(id)).size(12.5));
                            });
                        })
                        .response
                        .on_hover_text(format!("Conectado como {user}"));
                }
                if self.worker.busy() {
                    ui.add(egui::Spinner::new().size(16.0).color(p.accent));
                }
            });
        });
    }

    /// Avisos apilados en la esquina: entran deslizándose, muestran el tiempo que les queda y se
    /// desvanecen al caducar. Pasar el ratón por encima los detiene.
    fn show_toasts(&mut self, ui: &mut Ui) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        let p = theme::palette(ui);
        let ctx = ui.ctx().clone();
        let dt = Duration::from_secs_f32(ui.input(|i| i.stable_dt).min(0.1));
        let mut close = None;
        let mut y = 0.0;
        for (i, t) in self.toasts.iter_mut().enumerate().rev() {
            let age = now.duration_since(t.born).as_secs_f32();
            let left = t.until.saturating_duration_since(now).as_secs_f32();
            let enter = easing::cubic_out((age / TOAST_FADE).min(1.0));
            let exit = (left / TOAST_FADE).min(1.0);
            let id = egui::Id::unique(("toast", t.id));
            let y_off = theme::ease_to(ui, id.with("y"), y, 0.2);
            let (ic, color) = match t.kind {
                ToastKind::Success => (icon::CHECK_CIRCLE, p.success),
                ToastKind::Warning => (icon::WARNING, p.warning),
                ToastKind::Error => (icon::WARNING_OCTAGON, p.danger),
            };
            let area = egui::Area::new(id)
                .anchor(egui::Align2::RIGHT_BOTTOM, [-18.0 + 48.0 * (1.0 - enter), -18.0 - y_off])
                .order(egui::Order::Foreground)
                .show(&ctx, |ui| {
                    ui.set_max_width(420.0);
                    ui.multiply_opacity(enter.min(exit));
                    let frame = egui::Frame::new()
                        .fill(p.card)
                        .stroke(Stroke::new(1.0, color.gamma_multiply(0.5)))
                        .corner_radius(10)
                        .inner_margin(Margin { left: 14, right: 8, top: 10, bottom: 12 })
                        .shadow(ui.visuals().popup_shadow)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(ic).size(18.0).color(color));
                                ui.add(egui::Label::new(&t.text).wrap());
                                if theme::icon_button(ui, icon::X, "Cerrar").clicked() {
                                    close = Some(i);
                                }
                            });
                        });
                    let r = frame.response.rect;
                    let frac = (left / t.secs).clamp(0.0, 1.0);
                    let bar = Rect::from_min_size(pos2(r.left() + 12.0, r.bottom() - 5.0), vec2((r.width() - 24.0) * frac, 2.0));
                    ui.painter().rect_filled(bar, 1, color.gamma_multiply(0.55));
                });
            if area.response.contains_pointer() {
                t.until += dt;
            }
            y += area.response.rect.height() + 8.0;
        }
        if let Some(i) = close {
            self.toasts.remove(i);
        }
        ctx.request_repaint_after(Duration::from_millis(33));
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
                .show(ui, |ui| {
                    ui.multiply_opacity(theme::appear(ui, egui::Id::unique("screen_login"), 0.3));
                    login::show(self, ui)
                });
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
                .show(ui, |ui| {
                    ui.multiply_opacity(theme::appear(ui, egui::Id::unique(("screen", self.tab as u8)), 0.25));
                    match self.tab {
                        Tab::Explorer => explorer::show(self, ui),
                        Tab::Sync => sync_view::show(self, ui),
                        Tab::Mappings => mappings::show(self, ui),
                        Tab::Summary => summary::show(self, ui),
                        Tab::Settings => settings::show(self, ui),
                    }
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

/// Contenedor con margen y scroll para las páginas de contenido. Al abrirse, el contenido sube
/// unos píxeles mientras aparece.
pub fn page(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    let t = theme::appear(ui, egui::Id::unique(("page", id)), 0.3);
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new().inner_margin(Margin::symmetric(28, 22)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add_space(12.0 * (1.0 - t));
                add(ui)
            });
        });
}

pub fn weak(ui: &Ui, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(theme::palette(ui).text_weak)
}
