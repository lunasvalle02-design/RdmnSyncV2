//! Estado persistente: instancias configuradas, mapeos aprendidos y registro de sincronizaciones.
//! Se guarda como JSON legible en el directorio de configuración del usuario.
//! Las API keys NO se guardan aquí (ver `secrets`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::matching::normalize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    /// Identificador estable; también se usa en la marca `[sync:<id>#<n>]`.
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub insecure_tls: bool,
    #[serde(default)]
    pub remember_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLink {
    pub src_name: String,
    pub dst_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLink {
    pub src_name: String,
    pub dst_id: u32,
    pub dst_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkOrigin {
    /// Confirmado explícitamente por el usuario.
    Confirmed,
    /// Issue creado por la aplicación en el destino.
    Created,
    /// Emparejado automáticamente con alta confianza.
    Auto,
}

impl LinkOrigin {
    pub fn label(self) -> &'static str {
        match self {
            LinkOrigin::Confirmed => "Confirmado",
            LinkOrigin::Created => "Creado",
            LinkOrigin::Auto => "Automático",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueLink {
    pub src_subject: String,
    pub dst_id: u32,
    pub dst_subject: String,
    pub dst_project_id: u32,
    pub origin: LinkOrigin,
}

/// Regla: si el comentario de la imputación contiene `contains`, usar la actividad `dst_activity`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentRule {
    pub contains: String,
    pub dst_activity: String,
}

/// Mapeos dirigidos de un Redmine de origen a uno de destino.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PairMappings {
    /// Clave: nombre normalizado de la actividad de origen.
    #[serde(default)]
    pub activities: BTreeMap<String, ActivityLink>,
    /// Clave: id del proyecto de origen.
    #[serde(default)]
    pub projects: BTreeMap<u32, ProjectLink>,
    /// Clave: id del issue de origen.
    #[serde(default)]
    pub issues: BTreeMap<u32, IssueLink>,
    /// Tipos (trackers) y prioridades: nombre de origen → nombre de destino.
    #[serde(default)]
    pub trackers: BTreeMap<String, String>,
    #[serde(default)]
    pub priorities: BTreeMap<String, String>,
    #[serde(default)]
    pub rules: Vec<CommentRule>,
}

impl PairMappings {
    pub fn activity_for(&self, src_name: &str) -> Option<&str> {
        self.activities.get(&normalize(src_name)).map(|l| l.dst_name.as_str())
    }

    pub fn set_activity(&mut self, src_name: &str, dst_name: &str) {
        self.activities.insert(
            normalize(src_name),
            ActivityLink { src_name: src_name.to_string(), dst_name: dst_name.to_string() },
        );
    }

    /// Actividad forzada por una regla de comentario, si alguna aplica.
    pub fn rule_activity(&self, comments: &str) -> Option<&str> {
        let c = normalize(comments);
        self.rules
            .iter()
            .find(|r| !r.contains.trim().is_empty() && c.contains(&normalize(&r.contains)))
            .map(|r| r.dst_activity.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub pair: String,
    pub batch: String,
    pub src_entry_id: u32,
    pub dst_entry_id: u32,
    #[serde(default)]
    pub src_issue_id: Option<u32>,
    #[serde(default)]
    pub dst_issue_id: Option<u32>,
    pub hours: f64,
    pub spent_on: String,
    pub activity: String,
    pub synced_at: String,
    #[serde(default)]
    pub undone: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub dark_mode: bool,
    /// Añadir `[sync:origen#id]` al comentario en el destino.
    pub add_marker: bool,
    /// Umbral a partir del cual un emparejamiento de issue se acepta sin preguntar.
    pub auto_threshold: f32,
    /// Umbral a partir del cual se propone un candidato (por debajo se sugiere crear).
    pub ask_threshold: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { dark_mode: true, add_marker: true, auto_threshold: 0.92, ask_threshold: 0.55 }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub instances: Vec<Instance>,
    #[serde(default)]
    pub pairs: BTreeMap<String, PairMappings>,
    #[serde(default)]
    pub ledger: Vec<LedgerEntry>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(skip)]
    pub last_error: Option<String>,
}

pub fn pair_key(src: &str, dst: &str) -> String {
    format!("{src}->{dst}")
}

impl Store {
    pub fn path() -> PathBuf {
        directories::ProjectDirs::from("es", "RdmnSync", "RdmnSync")
            .map(|d| d.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rdmnsync.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                // No sobrescribir un fichero que no entendemos: guardar copia.
                let _ = std::fs::copy(&path, path.with_extension("json.bak"));
                Store {
                    last_error: Some(format!("No se pudo leer la configuración ({e}). Se ha guardado una copia .bak")),
                    ..Default::default()
                }
            }),
            Err(_) => Store::default(),
        }
    }

    pub fn save(&mut self) {
        let path = Self::path();
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
            std::fs::rename(&tmp, &path)
        })();
        self.last_error = result.err().map(|e| format!("No se pudo guardar la configuración: {e}"));
    }

    pub fn instance(&self, id: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.id == id)
    }

    pub fn instance_name(&self, id: &str) -> String {
        self.instance(id).map(|i| i.name.clone()).unwrap_or_else(|| id.to_string())
    }

    pub fn pair(&self, src: &str, dst: &str) -> PairMappings {
        self.pairs.get(&pair_key(src, dst)).cloned().unwrap_or_default()
    }

    pub fn pair_mut(&mut self, src: &str, dst: &str) -> &mut PairMappings {
        self.pairs.entry(pair_key(src, dst)).or_default()
    }

    /// Crea un id único y legible a partir del nombre.
    pub fn new_instance_id(&self, name: &str) -> String {
        let base: String = normalize(name).split_whitespace().collect::<Vec<_>>().join("-");
        let base = if base.is_empty() { "redmine".to_string() } else { base };
        let mut id = base.clone();
        let mut n = 2;
        while self.instance(&id).is_some() {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }

    pub fn remove_instance(&mut self, id: &str) {
        self.instances.retain(|i| i.id != id);
        let prefix = format!("{id}->");
        let suffix = format!("->{id}");
        self.pairs.retain(|k, _| !k.starts_with(&prefix) && !k.ends_with(&suffix));
    }

    /// Entradas vigentes del registro para una pareja, indexadas por imputación de origen.
    pub fn synced_map(&self, pair: &str) -> BTreeMap<u32, &LedgerEntry> {
        self.ledger
            .iter()
            .filter(|l| l.pair == pair && !l.undone)
            .map(|l| (l.src_entry_id, l))
            .collect()
    }
}
