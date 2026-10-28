//! Pantalla inicial: alta de Redmines y solicitud de API keys antes de usar la aplicación.

use std::collections::{BTreeMap, BTreeSet};

use egui::{RichText, TextEdit, Ui, vec2};

use crate::app::{App, Msg, Session, ToastKind, weak};
use crate::redmine::{Client, normalize_url};
use crate::secrets;
use crate::store::{Instance, Store};
use crate::theme::{self, icon};

pub enum ConnStatus {
    Connecting,
    Connected(String),
    Failed(String),
}

#[derive(Default)]
pub struct LoginState {
    keys: BTreeMap<String, String>,
    visible: BTreeSet<String>,
    status: BTreeMap<String, ConnStatus>,
    new_name: String,
    new_url: String,
    new_key: String,
    new_insecure: bool,
    new_remember: bool,
    show_add: bool,
}

impl LoginState {
    pub fn new(store: &Store) -> Self {
        let keys = store
            .instances
            .iter()
            .filter(|i| i.remember_key)
            .filter_map(|i| secrets::load(&i.id).map(|k| (i.id.clone(), k)))
            .collect();
        Self { keys, new_remember: true, show_add: store.instances.is_empty(), ..Default::default() }
    }
}

/// Conecta automáticamente las instancias con API key recordada.
pub fn autoconnect(app: &mut App) {
    let ids: Vec<String> = app.login.keys.keys().cloned().collect();
    for id in ids {
        connect(app, &id);
    }
}

pub fn connect(app: &mut App, inst_id: &str) {
    let Some(inst) = app.store.instance(inst_id).cloned() else { return };
    let key = app.login.keys.get(inst_id).cloned().unwrap_or_default();
    if key.trim().is_empty() {
        app.login.status.insert(inst.id.clone(), ConnStatus::Failed("Introduce la API key".into()));
        return;
    }
    app.login.status.insert(inst.id.clone(), ConnStatus::Connecting);
    app.worker.spawn(move || {
        let result = (|| {
            let client = Client::new(&inst.url, &key, inst.insecure_tls);
            let user = client.current_user().map_err(|e| e.to_string())?;
            let projects = client.my_projects(&user).map_err(|e| e.to_string())?;
            let activities = client.activities().unwrap_or_default();
            Ok(Box::new(Session { client, user, projects, activities }))
        })();
        Msg::Connected { inst: inst.id, key, result }
    });
}

pub fn on_connected(app: &mut App, inst: String, key: String, result: Result<Box<Session>, String>) {
    match result {
        Ok(session) => {
            let name = session.user.display_name();
            app.login.status.insert(inst.clone(), ConnStatus::Connected(name));
            app.sessions.insert(inst.clone(), *session);
            let remember = app.store.instance(&inst).is_some_and(|i| i.remember_key);
            if remember {
                if let Err(e) = secrets::save(&inst, &key) {
                    app.toast(ToastKind::Warning, e);
                }
            } else {
                secrets::delete(&inst);
            }
        }
        Err(e) => {
            app.sessions.remove(&inst);
            app.login.status.insert(inst, ConnStatus::Failed(e));
        }
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.set_max_width(720.0);
            ui.add_space(28.0);
            theme::logo(ui, 64.0);
            ui.add_space(10.0);
            ui.label(RichText::new("RdmnSync").size(30.0).family(theme::semibold()));
            ui.label(weak(ui, "Lleva tus imputaciones de un Redmine a otro sin teclearlas dos veces"));
            ui.add_space(22.0);

            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                instances_card(app, ui);
                ui.add_space(12.0);
                add_card(app, ui);
                ui.add_space(12.0);
                theme::callout(
                    ui,
                    icon::INFO,
                    p.info,
                    "Tu API key está en Redmine → «Mi cuenta» → «Clave de acceso a la API». \
                     El administrador debe tener activada la API REST. Las claves recordadas se \
                     guardan en el almacén de credenciales del sistema, nunca en texto plano.",
                );
                ui.add_space(16.0);
                footer(app, ui);
            });
        });
    });
}

fn instances_card(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui);
    let mut to_connect = Vec::new();
    let mut to_remove = None;
    let mut changed = false;
    theme::card(ui, |ui| {
        theme::section_title(ui, icon::PLUGS_CONNECTED, "Tus Redmines");
        if app.store.instances.is_empty() {
            ui.label(weak(ui, "Todavía no hay ninguno. Añade al menos dos para poder sincronizar."));
            return;
        }
        for inst in app.store.instances.iter_mut() {
            ui.add_space(6.0);
            let status = app.login.status.get(&inst.id);
            let color = match status {
                Some(ConnStatus::Connected(_)) => p.success,
                Some(ConnStatus::Failed(_)) => p.danger,
                Some(ConnStatus::Connecting) => p.warning,
                None => p.text_weak,
            };
            egui::Frame::new()
                .fill(p.surface)
                .stroke(egui::Stroke::new(1.0, p.border))
                .corner_radius(10)
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        theme::status_dot(ui, color);
                        ui.label(RichText::new(&inst.name).family(theme::semibold()));
                        ui.label(weak(ui, &inst.url));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::icon_button(ui, icon::TRASH, "Eliminar este Redmine").clicked() {
                                to_remove = Some(inst.id.clone());
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        let key = app.login.keys.entry(inst.id.clone()).or_default();
                        let visible = app.login.visible.contains(&inst.id);
                        let resp = ui.add(
                            TextEdit::singleline(key)
                                .password(!visible)
                                .hint_text(format!("{}  API key", icon::KEY))
                                .desired_width(300.0),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            to_connect.push(inst.id.clone());
                        }
                        let eye = if visible { icon::EYE_SLASH } else { icon::EYE };
                        if theme::icon_button(ui, eye, "Mostrar/ocultar").clicked() {
                            if visible {
                                app.login.visible.remove(&inst.id);
                            } else {
                                app.login.visible.insert(inst.id.clone());
                            }
                        }
                        if ui.checkbox(&mut inst.remember_key, "Recordar").changed() {
                            changed = true;
                            if !inst.remember_key {
                                secrets::delete(&inst.id);
                            }
                        }
                        let connecting = matches!(status, Some(ConnStatus::Connecting));
                        ui.add_enabled_ui(!connecting, |ui| {
                            let label = if matches!(status, Some(ConnStatus::Connected(_))) {
                                format!("{}  Reconectar", icon::ARROW_CLOCKWISE)
                            } else {
                                format!("{}  Conectar", icon::PLUG)
                            };
                            if ui.button(label).clicked() {
                                to_connect.push(inst.id.clone());
                            }
                        });
                        if connecting {
                            ui.spinner();
                        }
                    });
                    match status {
                        Some(ConnStatus::Connected(user)) => {
                            ui.label(RichText::new(format!("{}  Conectado como {user}", icon::CHECK)).color(p.success));
                        }
                        Some(ConnStatus::Failed(e)) => {
                            ui.label(RichText::new(format!("{}  {e}", icon::WARNING)).color(p.danger));
                        }
                        _ => {}
                    }
                });
        }
    });
    if changed {
        app.save();
    }
    for id in to_connect {
        connect(app, &id);
    }
    if let Some(id) = to_remove {
        secrets::delete(&id);
        app.sessions.remove(&id);
        app.login.status.remove(&id);
        app.login.keys.remove(&id);
        app.store.remove_instance(&id);
        app.save();
    }
}

fn add_card(app: &mut App, ui: &mut Ui) {
    let st = &mut app.login;
    let mut add = false;
    theme::card(ui, |ui| {
        ui.horizontal(|ui| {
            theme::section_title(ui, icon::PLUS_CIRCLE, "Añadir Redmine");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ic = if st.show_add { icon::CARET_UP } else { icon::CARET_DOWN };
                if theme::icon_button(ui, ic, "Mostrar formulario").clicked() {
                    st.show_add = !st.show_add;
                }
            });
        });
        if !st.show_add {
            return;
        }
        ui.add_space(6.0);
        egui::Grid::new("add_instance").num_columns(2).spacing(vec2(12.0, 10.0)).show(ui, |ui| {
            ui.label("Nombre");
            ui.add(TextEdit::singleline(&mut st.new_name).hint_text("p. ej. Redmine Cliente").desired_width(380.0));
            ui.end_row();
            ui.label("URL");
            ui.add(TextEdit::singleline(&mut st.new_url).hint_text("https://redmine.ejemplo.com").desired_width(380.0));
            ui.end_row();
            ui.label("API key");
            ui.add(TextEdit::singleline(&mut st.new_key).password(true).desired_width(380.0));
            ui.end_row();
            ui.label("");
            ui.vertical(|ui| {
                ui.checkbox(&mut st.new_remember, "Recordar la API key en este equipo");
                ui.checkbox(&mut st.new_insecure, "Aceptar certificados no válidos (solo redes internas)");
            });
            ui.end_row();
        });
        ui.add_space(4.0);
        let valid = !st.new_name.trim().is_empty() && !st.new_url.trim().is_empty() && !st.new_key.trim().is_empty();
        ui.add_enabled_ui(valid, |ui| {
            if theme::primary_button(ui, format!("{}  Añadir y conectar", icon::PLUS)).clicked() {
                add = true;
            }
        });
    });
    if add {
        let id = app.store.new_instance_id(&app.login.new_name);
        app.store.instances.push(Instance {
            id: id.clone(),
            name: app.login.new_name.trim().to_string(),
            url: normalize_url(&app.login.new_url),
            insecure_tls: app.login.new_insecure,
            remember_key: app.login.new_remember,
        });
        app.login.keys.insert(id.clone(), app.login.new_key.trim().to_string());
        app.login.new_name.clear();
        app.login.new_url.clear();
        app.login.new_key.clear();
        app.login.new_insecure = false;
        app.save();
        connect(app, &id);
    }
}

fn footer(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui);
    let connected = app.sessions.len();
    ui.horizontal(|ui| {
        if !app.store.instances.is_empty() && ui.button(format!("{}  Conectar todos", icon::PLUGS)).clicked() {
            let ids: Vec<String> = app.store.instances.iter().map(|i| i.id.clone()).collect();
            for id in ids {
                if !app.sessions.contains_key(&id) {
                    connect(app, &id);
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(connected > 0, |ui| {
                if theme::primary_button(ui, format!("Continuar  {}", icon::ARROW_RIGHT)).clicked() {
                    app.show_login = false;
                }
            });
            match connected {
                0 => {}
                1 => {
                    ui.label(RichText::new("Conecta al menos dos Redmines para sincronizar").color(p.warning));
                }
                n => {
                    ui.label(RichText::new(format!("{n} Redmines conectados")).color(p.success));
                }
            }
        });
    });
}
