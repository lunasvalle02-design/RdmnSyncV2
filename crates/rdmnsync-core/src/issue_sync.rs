//! Igualar issues de destino con los de origen: diferencias campo a campo y su aplicación.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Value, json};

use crate::dates;
use crate::matching::same_name;
use crate::redmine::{Client, Issue, IssueStatus, Priority, Version};
use crate::store::{IssueFields, PairMappings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Status,
    DoneRatio,
    StartDate,
    DueDate,
    Estimated,
    Version,
    Priority,
    Subject,
    Description,
}

impl Field {
    pub const ALL: [Field; 9] = [
        Field::Status,
        Field::DoneRatio,
        Field::StartDate,
        Field::DueDate,
        Field::Estimated,
        Field::Version,
        Field::Priority,
        Field::Subject,
        Field::Description,
    ];

    /// Clave estable para el registro de valores sincronizados.
    pub fn key(self) -> &'static str {
        match self {
            Field::Status => "status",
            Field::DoneRatio => "done_ratio",
            Field::StartDate => "start_date",
            Field::DueDate => "due_date",
            Field::Estimated => "estimated_hours",
            Field::Version => "fixed_version",
            Field::Priority => "priority",
            Field::Subject => "subject",
            Field::Description => "description",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Field::Status => "Estado",
            Field::DoneRatio => "% realizado",
            Field::StartDate => "Inicio",
            Field::DueDate => "Fin",
            Field::Estimated => "Estimación",
            Field::Version => "Versión",
            Field::Priority => "Prioridad",
            Field::Subject => "Asunto",
            Field::Description => "Descripción",
        }
    }

    /// Nombre del atributo en la API de Redmine.
    fn api_key(self) -> &'static str {
        match self {
            Field::Status => "status_id",
            Field::Version => "fixed_version_id",
            Field::Priority => "priority_id",
            other => other.key(),
        }
    }

    fn enabled(self, f: &IssueFields) -> bool {
        match self {
            Field::Status => f.status,
            Field::DoneRatio => f.done_ratio,
            Field::StartDate | Field::DueDate => f.dates,
            Field::Estimated => f.estimated,
            Field::Version => f.version,
            Field::Priority => f.priority,
            Field::Subject => f.subject,
            Field::Description => f.description,
        }
    }

    /// Valor actual del campo en un issue, en el formato que acepta la API.
    fn api_value(self, i: &Issue) -> Value {
        let opt = |s: &Option<String>| s.as_ref().map_or(Value::Null, |s| json!(s));
        match self {
            Field::Status => json!(i.status.id),
            Field::DoneRatio => json!(i.done_ratio.unwrap_or(0.0).round() as u32),
            Field::StartDate => opt(&i.start_date),
            Field::DueDate => opt(&i.due_date),
            Field::Estimated => i.estimated_hours.map_or(Value::Null, |h| json!(h)),
            Field::Version => i.fixed_version.as_ref().map_or(Value::Null, |v| json!(v.id)),
            Field::Priority => json!(i.priority.id),
            Field::Subject => json!(i.subject),
            Field::Description => json!(i.description_text()),
        }
    }

    /// Valor del origen tal como se recuerda entre sincronizaciones.
    fn raw(self, i: &Issue) -> String {
        match self {
            Field::Status => i.status.name.clone(),
            Field::Priority => i.priority.name.clone(),
            other => other.api_value(i).to_string(),
        }
    }

    fn display(self, i: &Issue) -> String {
        let none = || "—".to_string();
        match self {
            Field::Status => i.status.name.clone(),
            Field::Priority => i.priority.name.clone(),
            Field::Version => i.fixed_version.as_ref().map_or_else(none, |v| v.name.clone()),
            Field::DoneRatio => format!("{:.0} %", i.done_ratio.unwrap_or(0.0)),
            Field::StartDate => i.start_date.as_deref().map_or_else(none, dates::pretty),
            Field::DueDate => i.due_date.as_deref().map_or_else(none, dates::pretty),
            Field::Estimated => i.estimated_hours.map_or_else(none, crate::format::hours),
            Field::Subject => i.subject.clone(),
            Field::Description => short(i.description_text()),
        }
    }
}

fn short(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if line.is_empty() {
        "—".into()
    } else if line.chars().count() > 50 {
        line.chars().take(49).collect::<String>() + "…"
    } else {
        line.to_string()
    }
}

fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.as_f64().unwrap_or(0.0) - y.as_f64().unwrap_or(0.0)).abs() < 0.005
        }
        (Value::String(x), Value::String(y)) => x.trim().replace("\r\n", "\n") == y.trim().replace("\r\n", "\n"),
        (Value::String(s), Value::Null) | (Value::Null, Value::String(s)) => s.trim().is_empty(),
        _ => a == b,
    }
}

/// Algo que impide calcular un campo hasta que el usuario lo resuelva.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    /// Estado de origen sin equivalente conocido en el destino.
    Status(String),
    /// Versión de origen sin equivalente en el proyecto de destino.
    Version { src_id: u32, src_name: String, dst_project: u32 },
    /// Faltan por cargar las versiones del proyecto de destino.
    VersionsLoading(u32),
    Note(String),
}

#[derive(Debug, Clone)]
pub struct Change {
    pub field: Field,
    pub from: String,
    pub to: String,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub struct IssueDiff {
    pub src_id: u32,
    pub dst_id: u32,
    pub changes: Vec<Change>,
    pub pending: Vec<Pending>,
    /// Valores de origen de los campos resueltos, para recordarlos tras aplicar.
    pub snapshot: BTreeMap<String, String>,
    /// Versiones emparejadas por nombre idéntico (origen id, nombre, destino id, nombre).
    pub auto_versions: Vec<(u32, String, u32, String)>,
}

pub struct DiffCtx<'a> {
    pub pair: &'a PairMappings,
    pub fields: &'a IssueFields,
    pub respect_dst_changes: bool,
    pub dst_statuses: &'a [IssueStatus],
    pub dst_priorities: &'a [Priority],
    /// Versiones disponibles por proyecto de destino (solo las ya cargadas).
    pub dst_versions: &'a HashMap<u32, Vec<Version>>,
}

pub fn diff(src: &Issue, dst: &Issue, snapshot: Option<&BTreeMap<String, String>>, ctx: &DiffCtx) -> IssueDiff {
    let mut d = IssueDiff {
        src_id: src.id,
        dst_id: dst.id,
        changes: vec![],
        pending: vec![],
        snapshot: BTreeMap::new(),
        auto_versions: vec![],
    };
    for field in Field::ALL.into_iter().filter(|f| f.enabled(ctx.fields)) {
        let raw = field.raw(src);
        // El origen no ha cambiado desde la última vez: se respeta lo que haya en el destino.
        if ctx.respect_dst_changes && snapshot.and_then(|s| s.get(field.key())) == Some(&raw) {
            continue;
        }
        let Some((value, to)) = target(field, src, dst, ctx, &mut d) else { continue };
        d.snapshot.insert(field.key().to_string(), raw);
        if !same_value(&field.api_value(dst), &value) {
            d.changes.push(Change { field, from: field.display(dst), to, value });
        }
    }
    d
}

/// Valor que debe tener el campo en el destino, o `None` si aún no se puede saber.
fn target(field: Field, src: &Issue, dst: &Issue, ctx: &DiffCtx, d: &mut IssueDiff) -> Option<(Value, String)> {
    match field {
        Field::Status => {
            let wanted = ctx.pair.status_for(&src.status.name).unwrap_or(&src.status.name);
            match ctx.dst_statuses.iter().find(|s| same_name(&s.name, wanted)) {
                Some(s) => Some((json!(s.id), s.name.clone())),
                None => {
                    d.pending.push(Pending::Status(src.status.name.clone()));
                    None
                }
            }
        }
        Field::Priority => {
            let wanted = ctx.pair.priorities.get(&src.priority.name).map_or(src.priority.name.as_str(), String::as_str);
            match ctx.dst_priorities.iter().find(|p| same_name(&p.name, wanted)) {
                Some(p) => Some((json!(p.id), p.name.clone())),
                None => {
                    d.pending.push(Pending::Note(format!("La prioridad «{}» no existe en el destino", src.priority.name)));
                    None
                }
            }
        }
        Field::Version => {
            let Some(v) = &src.fixed_version else { return Some((Value::Null, "—".into())) };
            if let Some(l) = ctx.pair.versions.get(&v.id) {
                return Some((json!(l.dst_id), l.dst_name.clone()));
            }
            let Some(list) = ctx.dst_versions.get(&dst.project.id) else {
                d.pending.push(Pending::VersionsLoading(dst.project.id));
                return None;
            };
            match list.iter().find(|x| same_name(&x.name, &v.name)) {
                Some(x) => {
                    d.auto_versions.push((v.id, v.name.clone(), x.id, x.name.clone()));
                    Some((json!(x.id), x.name.clone()))
                }
                None => {
                    d.pending.push(Pending::Version { src_id: v.id, src_name: v.name.clone(), dst_project: dst.project.id });
                    None
                }
            }
        }
        Field::Description => {
            // Conserva el pie «Origen: …» que añade la aplicación al crear issues.
            let mut text = src.description_text().trim().to_string();
            if let Some(pos) = dst.description_text().find("---\nOrigen:") {
                text = format!("{text}\n\n{}", &dst.description_text()[pos..]).trim().to_string();
            }
            Some((json!(text), short(&text)))
        }
        other => Some((other.api_value(src), other.display(src))),
    }
}

pub fn payload(changes: &[Change]) -> Map<String, Value> {
    changes.iter().map(|c| (c.field.api_key().to_string(), c.value.clone())).collect()
}

/// Aplica los cambios y vuelve a leer el issue para detectar campos que Redmine ignoró
/// (por ejemplo, una transición de estado no permitida por el flujo de trabajo).
pub fn apply(client: &Client, dst_id: u32, changes: &[Change]) -> Result<(Issue, Vec<&'static str>), String> {
    client.update_issue(dst_id, &payload(changes)).map_err(|e| e.to_string())?;
    let issue = client.issue(dst_id).map_err(|e| e.to_string())?;
    let ignored = changes
        .iter()
        .filter(|c| !same_value(&c.field.api_value(&issue), &c.value))
        .map(|c| c.field.label())
        .collect();
    Ok((issue, ignored))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(status: (u32, &str), done: f32, version: Option<(u32, &str)>) -> Issue {
        serde_json::from_value(json!({
            "id": 1, "subject": "Login", "description": "Texto",
            "project": {"id": 5, "name": "P"},
            "tracker": {"id": 1, "name": "Tarea"},
            "status": {"id": status.0, "name": status.1},
            "priority": {"id": 2, "name": "Normal"},
            "done_ratio": done,
            "fixed_version": version.map(|(id, name)| json!({"id": id, "name": name})),
        }))
        .unwrap()
    }

    fn statuses() -> Vec<IssueStatus> {
        serde_json::from_value(json!([{"id": 1, "name": "Nueva"}, {"id": 2, "name": "En progreso"}])).unwrap()
    }

    fn versions() -> HashMap<u32, Vec<Version>> {
        let v: Vec<Version> =
            serde_json::from_value(json!([{"id": 70, "name": "Sprint 23"}])).unwrap();
        HashMap::from([(5, v)])
    }

    fn ctx<'a>(pair: &'a PairMappings, st: &'a [IssueStatus], v: &'a HashMap<u32, Vec<Version>>, f: &'a IssueFields) -> DiffCtx<'a> {
        DiffCtx { pair, fields: f, respect_dst_changes: true, dst_statuses: st, dst_priorities: &[], dst_versions: v }
    }

    #[test]
    fn unmapped_status_is_pending_and_mapped_is_changed() {
        let (st, v, f) = (statuses(), versions(), IssueFields::default());
        let src = issue((3, "En curso"), 50.0, Some((9, "Sprint 23")));
        let dst = issue((1, "Nueva"), 0.0, None);
        let mut pair = PairMappings::default();
        let d = diff(&src, &dst, None, &ctx(&pair, &st, &v, &f));
        assert!(d.pending.contains(&Pending::Status("En curso".into())));
        let fields: Vec<Field> = d.changes.iter().map(|c| c.field).collect();
        assert_eq!(fields, vec![Field::DoneRatio, Field::Version]);
        assert_eq!(d.auto_versions, vec![(9, "Sprint 23".into(), 70, "Sprint 23".into())]);

        pair.set_status("En curso", "En progreso");
        let d = diff(&src, &dst, None, &ctx(&pair, &st, &v, &f));
        assert!(d.pending.is_empty());
        assert_eq!(d.changes[0].value, json!(2));
        assert_eq!(payload(&d.changes).get("fixed_version_id"), Some(&json!(70)));
    }

    #[test]
    fn respects_destination_changes_when_source_did_not_change() {
        let (st, v, f) = (statuses(), versions(), IssueFields::default());
        let pair = PairMappings::default();
        let src = issue((1, "Nueva"), 50.0, None);
        let dst = issue((1, "Nueva"), 80.0, None); // alguien lo cambió en el destino
        let snapshot = BTreeMap::from([("done_ratio".to_string(), "50".to_string())]);
        let d = diff(&src, &dst, Some(&snapshot), &ctx(&pair, &st, &v, &f));
        assert!(d.changes.is_empty());

        let mut c = ctx(&pair, &st, &v, &f);
        c.respect_dst_changes = false;
        assert_eq!(diff(&src, &dst, Some(&snapshot), &c).changes.len(), 1);
    }

    #[test]
    fn description_keeps_origin_footer() {
        let (st, v) = (statuses(), versions());
        let f = IssueFields { description: true, ..IssueFields::default() };
        let pair = PairMappings::default();
        let src = issue((1, "Nueva"), 0.0, None);
        let mut dst = issue((1, "Nueva"), 0.0, None);
        dst.description = Some("Texto\n\n---\nOrigen: http://a/issues/1".into());
        assert!(diff(&src, &dst, None, &ctx(&pair, &st, &v, &f)).changes.is_empty());
    }
}
