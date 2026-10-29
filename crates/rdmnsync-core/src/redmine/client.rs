//! Cliente HTTP bloqueante para la API REST de Redmine.
//! La interfaz lo usa siempre desde hilos de fondo, nunca desde su hilo principal.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use ureq::Body;
use ureq::http::Response;
use ureq::tls::{RootCerts, TlsConfig};

use super::models::*;

const PAGE_SIZE: usize = 100;

#[derive(Debug, Clone)]
pub enum ApiError {
    Network(String),
    Http { status: u16, messages: Vec<String> },
    Decode(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Network(e) => write!(f, "Error de red: {e}"),
            ApiError::Decode(e) => write!(f, "Respuesta inesperada del servidor: {e}"),
            ApiError::Http { status, messages } => {
                let base = match status {
                    401 => "API key inválida o API REST deshabilitada en este Redmine",
                    403 => "No tienes permisos para esta operación",
                    404 => "Recurso no encontrado (¿URL correcta?)",
                    422 => "Redmine rechazó los datos",
                    s if *s >= 500 => "Error interno del servidor Redmine",
                    _ => "Error HTTP",
                };
                if messages.is_empty() {
                    write!(f, "{base} ({status})")
                } else {
                    write!(f, "{base} ({status}): {}", messages.join("; "))
                }
            }
        }
    }
}

impl std::error::Error for ApiError {}

pub type ApiResult<T> = Result<T, ApiError>;

fn network(e: ureq::Error) -> ApiError {
    ApiError::Network(e.to_string())
}

fn decode(e: serde_json::Error) -> ApiError {
    ApiError::Decode(e.to_string())
}

/// Normaliza la URL base: sin barra final y con esquema.
pub fn normalize_url(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

#[derive(Debug, Clone, Default)]
pub struct IssueQuery {
    pub project_id: Option<u32>,
    pub assigned_to_me: bool,
    pub include_closed: bool,
    pub ids: Vec<u32>,
    pub subject_contains: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Default)]
pub struct TimeEntryQuery {
    pub issue_id: Option<u32>,
    pub project_id: Option<u32>,
    pub only_me: bool,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: usize,
}

#[derive(Clone)]
pub struct Client {
    base: String,
    key: String,
    agent: ureq::Agent,
}

impl Client {
    pub fn new(url: &str, key: &str, insecure_tls: bool) -> Self {
        let tls = TlsConfig::builder()
            .root_certs(RootCerts::PlatformVerifier)
            .disable_verification(insecure_tls)
            .build();
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(45)))
            .user_agent(concat!("RdmnSync/", env!("CARGO_PKG_VERSION")))
            .tls_config(tls)
            .build();
        Self { base: normalize_url(url), key: key.trim().to_string(), agent: config.into() }
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub fn issue_url(&self, id: u32) -> String {
        format!("{}/issues/{id}", self.base)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> ApiResult<T> {
        let mut req = self
            .agent
            .get(self.url(path))
            .header("X-Redmine-API-Key", &self.key)
            .header("Accept", "application/json");
        for (k, v) in query {
            req = req.query(*k, v);
        }
        Self::read(req.call().map_err(network)?)
    }

    fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> ApiResult<T> {
        let resp = self
            .agent
            .post(self.url(path))
            .header("X-Redmine-API-Key", &self.key)
            .header("Accept", "application/json")
            .send_json(body)
            .map_err(network)?;
        Self::read(resp)
    }

    fn put(&self, path: &str, body: &Value) -> ApiResult<()> {
        let resp = self
            .agent
            .put(self.url(path))
            .header("X-Redmine-API-Key", &self.key)
            .header("Accept", "application/json")
            .send_json(body)
            .map_err(network)?;
        let _: Value = Self::read(resp)?;
        Ok(())
    }

    fn delete(&self, path: &str) -> ApiResult<()> {
        let resp = self
            .agent
            .delete(self.url(path))
            .header("X-Redmine-API-Key", &self.key)
            .call()
            .map_err(network)?;
        let _: Value = Self::read(resp)?;
        Ok(())
    }

    fn read<T: DeserializeOwned>(mut resp: Response<Body>) -> ApiResult<T> {
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .with_config()
            .limit(64 * 1024 * 1024)
            .read_to_string()
            .map_err(network)?;
        if status >= 400 {
            return Err(ApiError::Http { status, messages: parse_errors(&body) });
        }
        let body = if body.trim().is_empty() { "null" } else { body.as_str() };
        serde_json::from_str(body).map_err(decode)
    }

    /// Recorre todas las páginas de un listado (`limit`/`offset`), hasta `max` elementos.
    fn get_all<T: DeserializeOwned>(
        &self,
        path: &str,
        key: &str,
        query: &[(&str, String)],
        max: usize,
    ) -> ApiResult<Vec<T>> {
        let max = if max == 0 { usize::MAX } else { max };
        let mut out = Vec::new();
        let mut offset = 0usize;
        loop {
            let mut q = query.to_vec();
            q.push(("limit", PAGE_SIZE.min(max - out.len()).to_string()));
            q.push(("offset", offset.to_string()));
            let mut page: Value = self.get(path, &q)?;
            let items = page.get_mut(key).map(Value::take).unwrap_or(Value::Array(vec![]));
            let total = page.get("total_count").and_then(Value::as_u64).map(|t| t as usize);
            let items: Vec<T> = serde_json::from_value(items).map_err(decode)?;
            let n = items.len();
            out.extend(items);
            offset += n;
            let finished = match total {
                Some(t) => offset >= t,
                None => n < PAGE_SIZE,
            };
            if n == 0 || finished || out.len() >= max {
                break;
            }
        }
        Ok(out)
    }

    // ---- Consultas ---------------------------------------------------------

    pub fn current_user(&self) -> ApiResult<User> {
        #[derive(serde::Deserialize)]
        struct W {
            user: User,
        }
        let w: W = self.get("/users/current.json", &[("include", "memberships".into())])?;
        Ok(w.user)
    }

    /// Proyectos en los que el usuario es miembro, con información de jerarquía.
    pub fn my_projects(&self, user: &User) -> ApiResult<Vec<Project>> {
        let ids: std::collections::HashSet<u32> =
            user.memberships.iter().map(|m| m.project.id).collect();
        let all: Vec<Project> = self.get_all("/projects.json", "projects", &[], 0)?;
        let mut mine: Vec<Project> = all.into_iter().filter(|p| ids.contains(&p.id)).collect();
        // Si el listado global no los devuelve (permisos raros), usar los de las membresías.
        if mine.is_empty() && !ids.is_empty() {
            mine = user
                .memberships
                .iter()
                .map(|m| Project {
                    id: m.project.id,
                    name: m.project.name.clone(),
                    parent: None,
                    time_entry_activities: vec![],
                })
                .collect();
        }
        mine.sort_by_key(|p| p.name.to_lowercase());
        Ok(mine)
    }

    pub fn activities(&self) -> ApiResult<Vec<Activity>> {
        #[derive(serde::Deserialize)]
        struct W {
            time_entry_activities: Vec<Activity>,
        }
        let w: W = self.get("/enumerations/time_entry_activities.json", &[])?;
        Ok(w.time_entry_activities.into_iter().filter(|a| a.active).collect())
    }

    /// Actividades habilitadas en un proyecto (pueden diferir de las globales).
    pub fn project_activities(&self, project_id: u32) -> ApiResult<Vec<Activity>> {
        #[derive(serde::Deserialize)]
        struct W {
            project: Project,
        }
        let w: W = self.get(
            &format!("/projects/{project_id}.json"),
            &[("include", "time_entry_activities".into())],
        )?;
        if w.project.time_entry_activities.is_empty() {
            self.activities()
        } else {
            Ok(w.project.time_entry_activities)
        }
    }

    pub fn project_trackers(&self, project_id: u32) -> ApiResult<Vec<Tracker>> {
        #[derive(serde::Deserialize)]
        struct P {
            #[serde(default)]
            trackers: Vec<Tracker>,
        }
        #[derive(serde::Deserialize)]
        struct W {
            project: P,
        }
        let w: W =
            self.get(&format!("/projects/{project_id}.json"), &[("include", "trackers".into())])?;
        if w.project.trackers.is_empty() { self.trackers() } else { Ok(w.project.trackers) }
    }

    pub fn trackers(&self) -> ApiResult<Vec<Tracker>> {
        #[derive(serde::Deserialize)]
        struct W {
            trackers: Vec<Tracker>,
        }
        Ok(self.get::<W>("/trackers.json", &[])?.trackers)
    }

    pub fn statuses(&self) -> ApiResult<Vec<IssueStatus>> {
        #[derive(serde::Deserialize)]
        struct W {
            issue_statuses: Vec<IssueStatus>,
        }
        Ok(self.get::<W>("/issue_statuses.json", &[])?.issue_statuses)
    }

    /// Versiones utilizables en un proyecto (incluye las compartidas desde otros).
    pub fn versions(&self, project_id: u32) -> ApiResult<Vec<Version>> {
        #[derive(serde::Deserialize)]
        struct W {
            versions: Vec<Version>,
        }
        Ok(self.get::<W>(&format!("/projects/{project_id}/versions.json"), &[])?.versions)
    }

    pub fn priorities(&self) -> ApiResult<Vec<Priority>> {
        #[derive(serde::Deserialize)]
        struct W {
            issue_priorities: Vec<Priority>,
        }
        Ok(self.get::<W>("/enumerations/issue_priorities.json", &[])?.issue_priorities)
    }

    pub fn issues(&self, q: &IssueQuery) -> ApiResult<Vec<Issue>> {
        let mut query: Vec<(&str, String)> = vec![("sort", "updated_on:desc".into())];
        query.push(("status_id", if q.include_closed { "*" } else { "open" }.into()));
        if let Some(p) = q.project_id {
            query.push(("project_id", p.to_string()));
        }
        if q.assigned_to_me {
            query.push(("assigned_to_id", "me".into()));
        }
        if let Some(s) = &q.subject_contains {
            query.push(("subject", format!("~{s}")));
        }
        if !q.ids.is_empty() {
            let mut out = Vec::new();
            for chunk in q.ids.chunks(50) {
                let mut qq = query.clone();
                let ids: Vec<String> = chunk.iter().map(u32::to_string).collect();
                qq.push(("issue_id", ids.join(",")));
                out.extend(self.get_all::<Issue>("/issues.json", "issues", &qq, 0)?);
            }
            return Ok(out);
        }
        self.get_all("/issues.json", "issues", &query, q.limit)
    }

    pub fn issue(&self, id: u32) -> ApiResult<Issue> {
        #[derive(serde::Deserialize)]
        struct W {
            issue: Issue,
        }
        Ok(self.get::<W>(&format!("/issues/{id}.json"), &[])?.issue)
    }

    pub fn time_entries(&self, q: &TimeEntryQuery) -> ApiResult<Vec<TimeEntry>> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(i) = q.issue_id {
            query.push(("issue_id", i.to_string()));
        }
        if let Some(p) = q.project_id {
            query.push(("project_id", p.to_string()));
        }
        if q.only_me {
            query.push(("user_id", "me".into()));
        }
        if let Some(f) = &q.from {
            query.push(("from", f.clone()));
        }
        if let Some(t) = &q.to {
            query.push(("to", t.clone()));
        }
        self.get_all("/time_entries.json", "time_entries", &query, q.limit)
    }

    /// Campos personalizados que usan los issues de un proyecto/tipo, deducidos de un issue existente.
    /// (El listado `/custom_fields.json` solo está disponible para administradores.)
    pub fn sample_custom_fields(
        &self,
        project_id: u32,
        tracker_id: u32,
    ) -> ApiResult<Vec<CustomFieldValue>> {
        let q = [
            ("project_id", project_id.to_string()),
            ("tracker_id", tracker_id.to_string()),
            ("status_id", "*".to_string()),
            ("limit", "1".to_string()),
        ];
        let page: Value = self.get("/issues.json", &q)?;
        let issues: Vec<Issue> = serde_json::from_value(
            page.get("issues").cloned().unwrap_or(Value::Array(vec![])),
        )
        .map_err(decode)?;
        Ok(issues.into_iter().next().map(|i| i.custom_fields).unwrap_or_default())
    }

    // ---- Escritura ---------------------------------------------------------

    pub fn create_time_entry(&self, entry: &NewTimeEntry) -> ApiResult<TimeEntry> {
        #[derive(serde::Deserialize)]
        struct W {
            time_entry: TimeEntry,
        }
        let w: W = self.post("/time_entries.json", &json!({ "time_entry": entry }))?;
        Ok(w.time_entry)
    }

    /// Actualiza los atributos indicados (nombres de la API: `status_id`, `done_ratio`…).
    pub fn update_issue(&self, id: u32, fields: &serde_json::Map<String, Value>) -> ApiResult<()> {
        self.put(&format!("/issues/{id}.json"), &json!({ "issue": fields }))
    }

    pub fn delete_time_entry(&self, id: u32) -> ApiResult<()> {
        self.delete(&format!("/time_entries/{id}.json"))
    }

    pub fn create_issue(&self, issue: &NewIssue) -> ApiResult<Issue> {
        #[derive(serde::Deserialize)]
        struct W {
            issue: Issue,
        }
        let w: W = self.post("/issues.json", &json!({ "issue": issue }))?;
        Ok(w.issue)
    }
}

fn parse_errors(body: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return vec![];
    };
    match v.get("errors") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|e| e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string()))
            .collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => vec![],
    }
}
