//! Heurísticas de emparejamiento: similitud de textos, sugerencias de actividades
//! y ranking de issues candidatos en el Redmine de destino.

use std::collections::HashSet;

use crate::redmine::Issue;

/// Minúsculas, sin acentos, solo alfanuméricos separados por un espacio.
pub fn normalize(s: &str) -> String {
    let ascii = deunicode::deunicode(s).to_lowercase();
    let mut out = String::with_capacity(ascii.len());
    let mut last_space = true;
    for c in ascii.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim_end().to_string()
}

const STOPWORDS: &[&str] = &[
    "de", "la", "el", "en", "y", "a", "para", "por", "con", "del", "los", "las", "un", "una",
    "al", "se", "que", "o", "the", "of", "and", "to", "for", "in", "on", "an",
];

fn tokens(s: &str) -> Vec<String> {
    normalize(s)
        .split_whitespace()
        .filter(|t| !STOPWORDS.contains(t))
        .map(str::to_string)
        .collect()
}

/// Similitud entre 0 y 1 de dos textos cortos (asuntos, nombres).
pub fn similarity(a: &str, b: &str) -> f32 {
    let (na, nb) = (normalize(a), normalize(b));
    if na.is_empty() || nb.is_empty() {
        return 0.0;
    }
    if na == nb {
        return 1.0;
    }
    let ta: HashSet<String> = tokens(a).into_iter().collect();
    let tb: HashSet<String> = tokens(b).into_iter().collect();
    // Coincidencia "blanda" de palabras: oauth ≈ oauth2, exportacion ≈ exportaciones.
    let inter = ta.iter().filter(|x| tb.iter().any(|y| tokens_match(x, y))).count() as f64;
    let union = (ta.len() + tb.len()) as f64 - inter;
    let union = union.max(1.0);
    let jaccard = inter / union;
    let containment = inter / ta.len().min(tb.len()).max(1) as f64;

    let mut sa: Vec<&String> = ta.iter().collect();
    let mut sb: Vec<&String> = tb.iter().collect();
    sa.sort();
    sb.sort();
    let sorted_a = sa.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ");
    let sorted_b = sb.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ");
    let lev = strsim::normalized_levenshtein(&sorted_a, &sorted_b);
    let jw = strsim::jaro_winkler(&na, &nb);

    (0.35 * lev + 0.30 * jaccard + 0.15 * jw + 0.20 * containment).clamp(0.0, 0.99) as f32
}

fn tokens_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    (short.len() >= 4 && long.starts_with(short) && long.len() - short.len() <= 3)
        || (short.len() >= 5 && strsim::jaro_winkler(a, b) >= 0.93)
}

/// Familias de nombres habituales de actividades, para sugerir mapeos entre nombres distintos.
const ACTIVITY_FAMILIES: &[&[&str]] = &[
    &["desarrollo", "desarrollar", "codificar", "codificacion", "programar", "programacion", "implementacion", "implementar", "development", "develop", "coding", "construccion"],
    &["pruebas", "prueba", "probar", "testing", "test", "tests", "qa", "verificacion", "validacion", "calidad"],
    &["diseno", "disenar", "design", "arquitectura", "maquetacion"],
    &["analisis", "analizar", "analysis", "requisitos", "requerimientos", "especificacion"],
    &["reunion", "reuniones", "meeting", "meetings", "daily", "seguimiento"],
    &["gestion", "management", "coordinacion", "planificacion", "jefatura", "pm"],
    &["documentacion", "documentar", "documentation", "docs"],
    &["soporte", "support", "incidencias", "incidencia", "mantenimiento", "correctivo", "atencion"],
    &["formacion", "training", "aprendizaje", "curso", "cursos"],
    &["despliegue", "desplegar", "deploy", "deployment", "implantacion", "puesta", "release"],
    &["revision", "review", "code review", "revisar"],
];

fn family_of(name: &str) -> Option<usize> {
    let n = normalize(name);
    let words: Vec<&str> = n.split_whitespace().collect();
    ACTIVITY_FAMILIES
        .iter()
        .position(|fam| fam.iter().any(|w| n == *w || words.contains(w)))
}

/// Sugerencia para mapear un nombre (actividad, tipo, prioridad) contra una lista de destino.
/// Devuelve el índice elegido y su puntuación.
pub fn suggest_name(src: &str, candidates: &[String]) -> Option<(usize, f32)> {
    let src_family = family_of(src);
    candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut s = similarity(src, c);
            if src_family.is_some() && src_family == family_of(c) {
                s = s.max(0.85);
            }
            (i, s)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

pub fn same_name(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub issue_id: u32,
    pub subject: String,
    pub project_id: u32,
    pub project: String,
    pub status: String,
    pub score: f32,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    Sure,
    Doubtful,
    NotFound,
}

/// `true` si `text` contiene `url` seguida de un carácter que no es dígito (evita /issues/12 vs /issues/123).
fn mentions_url(text: &str, url: &str) -> bool {
    let mut rest = text;
    while let Some(pos) = rest.find(url) {
        let after = &rest[pos + url.len()..];
        if !after.starts_with(|c: char| c.is_ascii_digit()) {
            return true;
        }
        rest = after;
    }
    false
}

/// Ordena los issues del destino por probabilidad de corresponder a `src`.
pub fn rank_candidates(
    src: &Issue,
    src_issue_url: &str,
    dst_base_url: &str,
    pool: &[Issue],
    mapped_dst_project: Option<u32>,
    dst_user_id: u32,
) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = pool
        .iter()
        .map(|d| {
            let dst_url = format!("{dst_base_url}/issues/{}", d.id);
            let (score, reason) = if mentions_url(d.description_text(), src_issue_url)
                || mentions_url(&d.subject, src_issue_url)
            {
                (1.0, "El issue de destino referencia al de origen".to_string())
            } else if mentions_url(src.description_text(), &dst_url) {
                (1.0, "El issue de origen referencia a este".to_string())
            } else {
                let mut s = similarity(&src.subject, &d.subject);
                let mut why = vec![format!("asunto {:.0}% similar", s * 100.0)];
                if mapped_dst_project == Some(d.project.id) {
                    s += 0.04;
                    why.push("proyecto mapeado".into());
                }
                if d.assigned_to.as_ref().map(|a| a.id) == Some(dst_user_id) {
                    s += 0.03;
                    why.push("asignado a ti".into());
                }
                if same_name(&src.tracker.name, &d.tracker.name) {
                    s += 0.01;
                }
                (s.min(0.97), why.join(", "))
            };
            Candidate {
                issue_id: d.id,
                subject: d.subject.clone(),
                project_id: d.project.id,
                project: d.project.name.clone(),
                status: d.status.name.clone(),
                score,
                reason,
            }
        })
        .collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out.dedup_by_key(|c| c.issue_id);
    out.truncate(8);
    out
}

pub fn classify(cands: &[Candidate], auto: f32, ask: f32) -> Confidence {
    let Some(top) = cands.first() else { return Confidence::NotFound };
    if top.score >= 1.0 {
        return Confidence::Sure;
    }
    let ambiguous = cands.get(1).is_some_and(|s| top.score - s.score < 0.05);
    if top.score >= auto && !ambiguous {
        Confidence::Sure
    } else if top.score >= ask {
        Confidence::Doubtful
    } else {
        Confidence::NotFound
    }
}

/// Palabra más representativa de un asunto, para buscar candidatos en el destino.
pub fn search_keyword(subject: &str) -> Option<String> {
    tokens(subject)
        .into_iter()
        .filter(|t| t.len() >= 4 && !t.chars().all(|c| c.is_ascii_digit()))
        .max_by_key(|t| t.len())
}

/// Marca que se añade al comentario en el destino.
pub fn sync_marker(src_instance: &str, src_entry_id: u32) -> String {
    format!("[sync:{src_instance}#{src_entry_id}]")
}

/// Extrae el id de imputación de origen de una marca `[sync:<instancia>#<id>]`.
pub fn parse_marker(comments: &str, src_instance: &str) -> Option<u32> {
    let prefix = format!("[sync:{src_instance}#");
    let start = comments.find(&prefix)? + prefix.len();
    let digits: String = comments[start..].chars().take_while(char::is_ascii_digit).collect();
    if comments[start + digits.len()..].starts_with(']') { digits.parse().ok() } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_accents_and_punctuation() {
        assert_eq!(normalize("  Diseño   de la API (v2)! "), "diseno de la api v2");
    }

    #[test]
    fn similarity_orders_sensibly() {
        let a = "Implementar login con OAuth";
        assert!(similarity(a, "Implementar el login con OAuth2") > similarity(a, "Revisar facturas"));
        assert_eq!(similarity("Pruebas", "pruebas"), 1.0);
        assert!(similarity("Implementar login con OAuth", "Implementar el login con OAuth2") > 0.85);
    }

    #[test]
    fn activity_families() {
        let c = vec!["Codificar".to_string(), "Reuniones".to_string(), "Testing".to_string()];
        assert_eq!(suggest_name("Desarrollar", &c).unwrap().0, 0);
        assert_eq!(suggest_name("Pruebas", &c).unwrap().0, 2);
        assert_eq!(suggest_name("Reunión", &c).unwrap().0, 1);
    }

    #[test]
    fn marker_roundtrip() {
        let m = format!("Hecho algo {}", sync_marker("cliente", 4521));
        assert_eq!(parse_marker(&m, "cliente"), Some(4521));
        assert_eq!(parse_marker(&m, "otro"), None);
        assert_eq!(parse_marker("[sync:cliente#12", "cliente"), None);
    }

    #[test]
    fn url_mentions_respect_boundaries() {
        assert!(mentions_url("ver https://r/issues/12.", "https://r/issues/12"));
        assert!(!mentions_url("ver https://r/issues/123", "https://r/issues/12"));
    }
}
