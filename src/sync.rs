//! Motor de sincronización: carga de datos de ambos Redmines, construcción del plan
//! (qué imputaciones hay que crear, con qué issue y actividad) y ejecución.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::matching::{self, Candidate, Confidence};
use crate::redmine::{
    Activity, Client, Issue, IssueQuery, NewTimeEntry, Project, TimeEntry, TimeEntryQuery,
};
use crate::store::{LedgerEntry, LinkOrigin, PairMappings, ProjectLink, Settings};

/// Longitud máxima del comentario de una imputación en Redmine.
const MAX_COMMENT: usize = 1024;

pub struct LoadRequest {
    pub src: Client,
    pub dst: Client,
    pub from: String,
    pub to: String,
    pub src_project: Option<u32>,
    pub mappings: PairMappings,
}

/// Datos en bruto de ambos lados necesarios para construir el plan.
#[derive(Clone)]
pub struct SyncData {
    pub entries: Vec<TimeEntry>,
    pub src_issues: HashMap<u32, Issue>,
    pub dst_entries: Vec<TimeEntry>,
    pub dst_pool: Vec<Issue>,
    pub dst_activities: Vec<Activity>,
}

pub fn load(req: LoadRequest) -> Result<SyncData, String> {
    let LoadRequest { src, dst, from, to, src_project, mappings } = req;
    let e = |e: crate::redmine::ApiError| e.to_string();

    let mut entries = src
        .time_entries(&TimeEntryQuery {
            only_me: true,
            project_id: src_project,
            from: Some(from.clone()),
            to: Some(to.clone()),
            ..Default::default()
        })
        .map_err(|err| format!("Origen: {err}"))?;
    entries.sort_by(|a, b| a.spent_on.cmp(&b.spent_on).then(a.id.cmp(&b.id)));

    let issue_ids: Vec<u32> = entries
        .iter()
        .filter_map(TimeEntry::issue_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let src_issues: HashMap<u32, Issue> = if issue_ids.is_empty() {
        HashMap::new()
    } else {
        src.issues(&IssueQuery { ids: issue_ids, include_closed: true, ..Default::default() })
            .map_err(|err| format!("Origen: {err}"))?
            .into_iter()
            .map(|i| (i.id, i))
            .collect()
    };

    let dst_entries = dst
        .time_entries(&TimeEntryQuery {
            only_me: true,
            from: Some(from),
            to: Some(to),
            ..Default::default()
        })
        .map_err(|err| format!("Destino: {err}"))?;

    // Conjunto de issues candidatos en el destino.
    let mut pool: Vec<Issue> = dst
        .issues(&IssueQuery { assigned_to_me: true, include_closed: true, limit: 300, ..Default::default() })
        .map_err(|err| format!("Destino: {}", e(err)))?;

    let linked: Vec<u32> = src_issues
        .keys()
        .filter_map(|id| mappings.issues.get(id).map(|l| l.dst_id))
        .collect();
    if !linked.is_empty() {
        // Puede fallar si un issue enlazado se borró; no es fatal.
        if let Ok(found) =
            dst.issues(&IssueQuery { ids: linked, include_closed: true, ..Default::default() })
        {
            pool.extend(found);
        }
    }

    let mapped_projects: HashSet<u32> = src_issues
        .values()
        .map(|i| i.project.id)
        .chain(entries.iter().map(|t| t.project.id))
        .filter_map(|p| mappings.projects.get(&p).map(|l| l.dst_id))
        .collect();
    for p in mapped_projects {
        if let Ok(found) = dst.issues(&IssueQuery {
            project_id: Some(p),
            include_closed: true,
            limit: 300,
            ..Default::default()
        }) {
            pool.extend(found);
        }
    }

    // Búsqueda por palabra clave para los issues que aún no tienen enlace.
    let mut searched = HashSet::new();
    for issue in src_issues.values().filter(|i| !mappings.issues.contains_key(&i.id)) {
        if let Some(kw) = matching::search_keyword(&issue.subject)
            && searched.insert(kw.clone())
            && let Ok(found) = dst.issues(&IssueQuery {
                subject_contains: Some(kw),
                include_closed: true,
                limit: 25,
                ..Default::default()
            })
        {
            pool.extend(found);
        }
    }
    let mut seen = HashSet::new();
    pool.retain(|i| seen.insert(i.id));

    let dst_activities = dst.activities().map_err(|err| format!("Destino: {err}"))?;

    Ok(SyncData { entries, src_issues, dst_entries, dst_pool: pool, dst_activities })
}

// ---- Plan -----------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum IssueState {
    Linked { dst_id: u32, subject: String, project_id: u32, project: String, origin: LinkOrigin, score: f32 },
    Pending(Confidence),
    Skipped,
}

pub struct IssueRes {
    pub src: Issue,
    pub state: IssueState,
    pub candidates: Vec<Candidate>,
    /// Selección actual en la interfaz (índice en `candidates`).
    pub choice: usize,
    /// Texto del campo "otro issue" de la interfaz.
    pub manual: String,
}

pub struct ProjectRes {
    pub src_name: String,
    pub dst: Option<(u32, String)>,
    pub confirmed: bool,
}

pub struct ActivityRes {
    pub dst: Option<String>,
    pub confirmed: bool,
    /// Mapeo nuevo (no estaba guardado) que conviene persistir.
    pub is_new: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowSync {
    New,
    Already { dst_entry_id: u32 },
    Duplicate { dst_entry_id: u32 },
    Done { dst_entry_id: u32 },
    Failed(String),
}

pub struct Row {
    pub entry: TimeEntry,
    pub include: bool,
    pub sync: RowSync,
    /// Actividad de destino forzada por una regla de comentario.
    pub rule_activity: Option<String>,
}

/// Mapeo de proyecto deducido de los issues enlazados.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSuggestion {
    pub src_id: u32,
    pub src_name: String,
    pub dst_id: u32,
    pub dst_name: String,
    /// Issues enlazados del proyecto de origen que están en `dst_id`.
    pub agreeing: usize,
    /// Issues enlazados del proyecto de origen en total.
    pub linked: usize,
}

pub enum Readiness {
    Ready { dst_issue: Option<u32>, dst_project: u32, activity: String },
    Blocked(String),
    Finished,
}

pub struct Plan {
    pub src_id: String,
    pub src_base: String,
    pub dst_base: String,
    pub data: SyncData,
    pub rows: Vec<Row>,
    pub issues: BTreeMap<u32, IssueRes>,
    pub projects: BTreeMap<u32, ProjectRes>,
    pub activities: BTreeMap<String, ActivityRes>,
}

pub struct PlanInput<'a> {
    pub src_id: &'a str,
    pub src_base: &'a str,
    pub dst_base: &'a str,
    pub dst_user_id: u32,
    pub dst_projects: &'a [Project],
    pub mappings: &'a PairMappings,
    pub ledger: &'a BTreeMap<u32, &'a LedgerEntry>,
    pub settings: &'a Settings,
}

impl Plan {
    pub fn build(data: SyncData, input: PlanInput) -> Plan {
        let m = input.mappings;
        let pool_by_id: HashMap<u32, &Issue> = data.dst_pool.iter().map(|i| (i.id, i)).collect();

        let mut issues = BTreeMap::new();
        for (id, src) in &data.src_issues {
            let src_url = format!("{}/issues/{id}", input.src_base);
            let mapped_project = m.projects.get(&src.project.id).map(|l| l.dst_id);
            let candidates = matching::rank_candidates(
                src,
                &src_url,
                input.dst_base,
                &data.dst_pool,
                mapped_project,
                input.dst_user_id,
            );
            let state = if let Some(link) = m.issues.get(id) {
                let (project_id, project) = pool_by_id
                    .get(&link.dst_id)
                    .map(|i| (i.project.id, i.project.name.clone()))
                    .unwrap_or((link.dst_project_id, String::new()));
                IssueState::Linked {
                    dst_id: link.dst_id,
                    subject: link.dst_subject.clone(),
                    project_id,
                    project,
                    origin: link.origin,
                    score: 1.0,
                }
            } else {
                match matching::classify(
                    &candidates,
                    input.settings.auto_threshold,
                    input.settings.ask_threshold,
                ) {
                    Confidence::Sure => {
                        let c = &candidates[0];
                        IssueState::Linked {
                            dst_id: c.issue_id,
                            subject: c.subject.clone(),
                            project_id: c.project_id,
                            project: c.project.clone(),
                            origin: LinkOrigin::Auto,
                            score: c.score,
                        }
                    }
                    other => IssueState::Pending(other),
                }
            };
            issues.insert(
                *id,
                IssueRes { src: src.clone(), state, candidates, choice: 0, manual: String::new() },
            );
        }

        // Proyectos: solo hacen falta para imputaciones sin issue.
        let mut projects = BTreeMap::new();
        for e in data.entries.iter().filter(|e| e.issue.is_none()) {
            projects.entry(e.project.id).or_insert_with(|| {
                if let Some(link) = m.projects.get(&e.project.id) {
                    ProjectRes {
                        src_name: e.project.name.clone(),
                        dst: Some((link.dst_id, link.dst_name.clone())),
                        confirmed: true,
                    }
                } else {
                    let names: Vec<String> =
                        input.dst_projects.iter().map(|p| p.name.clone()).collect();
                    let dst = matching::suggest_name(&e.project.name, &names)
                        .filter(|(_, s)| *s >= 0.5)
                        .map(|(i, _)| (input.dst_projects[i].id, input.dst_projects[i].name.clone()));
                    ProjectRes { src_name: e.project.name.clone(), dst, confirmed: false }
                }
            });
        }

        // Actividades: idéntico nombre => automático; si no, sugerencia y confirmación.
        let dst_names: Vec<String> = data.dst_activities.iter().map(|a| a.name.clone()).collect();
        let mut activities = BTreeMap::new();
        for e in &data.entries {
            let name = e.activity.name.clone();
            activities.entry(name.clone()).or_insert_with(|| {
                if let Some(dst) = m.activity_for(&name) {
                    ActivityRes { dst: Some(dst.to_string()), confirmed: true, is_new: false }
                } else if let Some(same) = dst_names.iter().find(|d| matching::same_name(d, &name)) {
                    ActivityRes { dst: Some(same.clone()), confirmed: true, is_new: true }
                } else {
                    let dst = matching::suggest_name(&name, &dst_names)
                        .filter(|(_, s)| *s >= 0.5)
                        .map(|(i, _)| dst_names[i].clone());
                    ActivityRes { dst, confirmed: false, is_new: true }
                }
            });
        }

        let dst_ids: HashSet<u32> = data.dst_entries.iter().map(|e| e.id).collect();
        let rows = data
            .entries
            .iter()
            .map(|entry| {
                let marked = data
                    .dst_entries
                    .iter()
                    .find(|d| matching::parse_marker(d.comments_text(), input.src_id) == Some(entry.id));
                let sync = if let Some(d) = marked {
                    RowSync::Already { dst_entry_id: d.id }
                } else if let Some(l) =
                    input.ledger.get(&entry.id).filter(|l| dst_ids.contains(&l.dst_entry_id))
                {
                    RowSync::Already { dst_entry_id: l.dst_entry_id }
                } else {
                    RowSync::New
                };
                Row {
                    include: sync == RowSync::New,
                    rule_activity: m.rule_activity(entry.comments_text()).map(str::to_string),
                    entry: entry.clone(),
                    sync,
                }
            })
            .collect();

        let mut plan = Plan {
            src_id: input.src_id.to_string(),
            src_base: input.src_base.to_string(),
            dst_base: input.dst_base.to_string(),
            data,
            rows,
            issues,
            projects,
            activities,
        };
        plan.refresh_duplicates();
        plan
    }

    fn dst_issue_of(&self, entry: &TimeEntry) -> Option<u32> {
        let id = entry.issue_id()?;
        match &self.issues.get(&id)?.state {
            IssueState::Linked { dst_id, .. } => Some(*dst_id),
            _ => None,
        }
    }

    /// Detecta imputaciones que ya parecen existir en el destino (mismo día, horas e issue)
    /// aunque no tengan marca ni registro. Se recalcula al cambiar enlaces de issues.
    pub fn refresh_duplicates(&mut self) {
        let marked: HashSet<u32> = self
            .data
            .dst_entries
            .iter()
            .filter(|d| d.comments_text().contains("[sync:"))
            .map(|d| d.id)
            .collect();
        let mut used: HashSet<u32> = self
            .rows
            .iter()
            .filter_map(|r| match r.sync {
                RowSync::Already { dst_entry_id } | RowSync::Done { dst_entry_id } => Some(dst_entry_id),
                _ => None,
            })
            .collect();
        for i in 0..self.rows.len() {
            let row = &self.rows[i];
            if !matches!(row.sync, RowSync::New | RowSync::Duplicate { .. }) {
                continue;
            }
            let dst_issue = self.dst_issue_of(&row.entry);
            let dup = self.data.dst_entries.iter().find(|d| {
                !marked.contains(&d.id)
                    && !used.contains(&d.id)
                    && d.spent_on == row.entry.spent_on
                    && (d.hours - row.entry.hours).abs() < 0.005
                    && match (dst_issue, row.entry.issue_id()) {
                        (Some(di), _) => d.issue_id() == Some(di),
                        (None, None) => d.issue.is_none(),
                        (None, Some(_)) => false,
                    }
            });
            let was = self.rows[i].sync.clone();
            match dup {
                Some(d) => {
                    used.insert(d.id);
                    if !matches!(was, RowSync::Duplicate { .. }) {
                        self.rows[i].include = false;
                    }
                    self.rows[i].sync = RowSync::Duplicate { dst_entry_id: d.id };
                }
                None => {
                    if matches!(was, RowSync::Duplicate { .. }) {
                        self.rows[i].include = true;
                    }
                    self.rows[i].sync = RowSync::New;
                }
            }
        }
    }

    /// Sugiere mapeos de proyecto para los proyectos de origen aún sin mapear: el proyecto de
    /// destino donde está la mayoría (estricta) de sus issues enlazados.
    pub fn project_suggestions(
        &self,
        known: &BTreeMap<u32, ProjectLink>,
        dst_projects: &[Project],
    ) -> Vec<ProjectSuggestion> {
        let mut votes: BTreeMap<u32, (String, BTreeMap<u32, (usize, String)>)> = BTreeMap::new();
        for res in self.issues.values() {
            if let IssueState::Linked { project_id, project, .. } = &res.state {
                let (_, by_dst) = votes
                    .entry(res.src.project.id)
                    .or_insert_with(|| (res.src.project.name.clone(), BTreeMap::new()));
                let slot = by_dst.entry(*project_id).or_insert((0, project.clone()));
                slot.0 += 1;
            }
        }
        votes
            .into_iter()
            .filter(|(src_id, _)| {
                !known.contains_key(src_id) && !self.projects.get(src_id).is_some_and(|r| r.confirmed)
            })
            .filter_map(|(src_id, (src_name, by_dst))| {
                let linked: usize = by_dst.values().map(|(n, _)| n).sum();
                let (dst_id, (agreeing, name)) = by_dst.into_iter().max_by_key(|(_, (n, _))| *n)?;
                if agreeing * 2 <= linked {
                    return None;
                }
                let dst_name = dst_projects
                    .iter()
                    .find(|p| p.id == dst_id)
                    .map(|p| p.name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or(name);
                Some(ProjectSuggestion { src_id, src_name, dst_id, dst_name, agreeing, linked })
            })
            .collect()
    }

    /// Aplica al plan un mapeo de proyecto ya confirmado.
    pub fn set_project(&mut self, src_id: u32, dst_id: u32, dst_name: &str) {
        if let Some(res) = self.projects.get_mut(&src_id) {
            res.dst = Some((dst_id, dst_name.to_string()));
            res.confirmed = true;
        }
    }

    pub fn activity_for_row(&self, row: &Row) -> Option<String> {
        if let Some(a) = &row.rule_activity {
            return Some(a.clone());
        }
        let res = self.activities.get(&row.entry.activity.name)?;
        if res.confirmed { res.dst.clone() } else { None }
    }

    pub fn readiness(&self, row: &Row) -> Readiness {
        if matches!(row.sync, RowSync::Already { .. } | RowSync::Done { .. }) {
            return Readiness::Finished;
        }
        let Some(activity) = self.activity_for_row(row) else {
            return Readiness::Blocked(format!("Falta mapear la actividad «{}»", row.entry.activity.name));
        };
        match row.entry.issue_id() {
            Some(id) => match self.issues.get(&id).map(|r| &r.state) {
                Some(IssueState::Linked { dst_id, project_id, .. }) => {
                    Readiness::Ready { dst_issue: Some(*dst_id), dst_project: *project_id, activity }
                }
                Some(IssueState::Skipped) => Readiness::Blocked("Issue omitido".into()),
                Some(IssueState::Pending(_)) => Readiness::Blocked("Issue de destino sin confirmar".into()),
                None => Readiness::Blocked(format!("No se pudo leer el issue #{id} del origen")),
            },
            None => match self.projects.get(&row.entry.project.id) {
                Some(ProjectRes { dst: Some((pid, _)), confirmed: true, .. }) => {
                    Readiness::Ready { dst_issue: None, dst_project: *pid, activity }
                }
                _ => Readiness::Blocked("Falta el proyecto de destino".into()),
            },
        }
    }

    pub fn link_issue(&mut self, src_issue: u32, issue: &Issue, origin: LinkOrigin) {
        if let Some(res) = self.issues.get_mut(&src_issue) {
            res.state = IssueState::Linked {
                dst_id: issue.id,
                subject: issue.subject.clone(),
                project_id: issue.project.id,
                project: issue.project.name.clone(),
                origin,
                score: 1.0,
            };
        }
        if !self.data.dst_pool.iter().any(|i| i.id == issue.id) {
            self.data.dst_pool.push(issue.clone());
        }
        self.refresh_duplicates();
    }

    pub fn unlink_issue(&mut self, src_issue: u32) {
        if let Some(res) = self.issues.get_mut(&src_issue) {
            res.state = IssueState::Pending(Confidence::Doubtful);
        }
        self.refresh_duplicates();
    }

    pub fn build_items(&self, add_marker: bool) -> Vec<ExecItem> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.include && !matches!(r.sync, RowSync::Already { .. } | RowSync::Done { .. }))
            .filter_map(|(idx, r)| match self.readiness(r) {
                Readiness::Ready { dst_issue, dst_project, activity } => {
                    let mut comments = r.entry.comments_text().trim().to_string();
                    if add_marker {
                        let marker = matching::sync_marker(&self.src_id, r.entry.id);
                        let room = MAX_COMMENT.saturating_sub(marker.len() + 1);
                        if comments.chars().count() > room {
                            comments = comments.chars().take(room.saturating_sub(1)).collect::<String>() + "…";
                        }
                        comments = if comments.is_empty() { marker } else { format!("{comments} {marker}") };
                    } else if comments.chars().count() > MAX_COMMENT {
                        comments = comments.chars().take(MAX_COMMENT).collect();
                    }
                    Some(ExecItem {
                        row: idx,
                        src_entry_id: r.entry.id,
                        src_issue_id: r.entry.issue_id(),
                        activity_name: activity,
                        dst_project,
                        new: NewTimeEntry {
                            issue_id: dst_issue,
                            project_id: if dst_issue.is_none() { Some(dst_project) } else { None },
                            spent_on: r.entry.spent_on.clone(),
                            hours: r.entry.hours,
                            activity_id: 0,
                            comments,
                        },
                    })
                }
                _ => None,
            })
            .collect()
    }
}

// ---- Ejecución ------------------------------------------------------------

#[derive(Clone)]
pub struct ExecItem {
    pub row: usize,
    pub src_entry_id: u32,
    pub src_issue_id: Option<u32>,
    pub activity_name: String,
    pub dst_project: u32,
    pub new: NewTimeEntry,
}

/// Crea las imputaciones en el destino, resolviendo el id de actividad por proyecto
/// (cada proyecto puede tener su propio juego de actividades).
pub fn execute(
    dst: &Client,
    items: Vec<ExecItem>,
    mut progress: impl FnMut(&ExecItem, Result<TimeEntry, String>),
) {
    let mut cache: HashMap<u32, Vec<Activity>> = HashMap::new();
    for mut item in items {
        let acts = cache.entry(item.dst_project).or_insert_with(|| {
            dst.project_activities(item.dst_project)
                .or_else(|_| dst.activities())
                .unwrap_or_default()
        });
        let result = match acts.iter().find(|a| matching::same_name(&a.name, &item.activity_name)) {
            None => Err(format!(
                "La actividad «{}» no está habilitada en el proyecto de destino",
                item.activity_name
            )),
            Some(a) => {
                item.new.activity_id = a.id;
                dst.create_time_entry(&item.new).map_err(|e| e.to_string())
            }
        };
        progress(&item, result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(id: u32, project: u32) -> Issue {
        serde_json::from_value(serde_json::json!({
            "id": id, "subject": format!("Issue {id}"),
            "project": {"id": project, "name": format!("Origen {project}")},
            "tracker": {"id": 1, "name": "Tarea"}, "status": {"id": 1, "name": "Nueva"},
            "priority": {"id": 2, "name": "Normal"}
        }))
        .unwrap()
    }

    fn plan_with(links: &[(u32, u32, Option<u32>)]) -> Plan {
        // (issue de origen, proyecto de origen, proyecto de destino si está enlazado)
        let issues = links
            .iter()
            .map(|&(id, src_p, dst_p)| {
                let state = match dst_p {
                    Some(p) => IssueState::Linked {
                        dst_id: id + 100,
                        subject: String::new(),
                        project_id: p,
                        project: format!("Destino {p}"),
                        origin: LinkOrigin::Confirmed,
                        score: 1.0,
                    },
                    None => IssueState::Pending(Confidence::NotFound),
                };
                (id, IssueRes { src: issue(id, src_p), state, candidates: vec![], choice: 0, manual: String::new() })
            })
            .collect();
        Plan {
            src_id: "a".into(),
            src_base: String::new(),
            dst_base: String::new(),
            data: SyncData {
                entries: vec![],
                src_issues: HashMap::new(),
                dst_entries: vec![],
                dst_pool: vec![],
                dst_activities: vec![],
            },
            rows: vec![],
            issues,
            projects: BTreeMap::new(),
            activities: BTreeMap::new(),
        }
    }

    #[test]
    fn suggests_majority_project() {
        let plan = plan_with(&[(1, 10, Some(7)), (2, 10, Some(7)), (3, 10, Some(8)), (4, 10, None)]);
        let s = plan.project_suggestions(&BTreeMap::new(), &[]);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].src_id, s[0].dst_id, s[0].agreeing, s[0].linked), (10, 7, 2, 3));
        assert_eq!(s[0].dst_name, "Destino 7");
    }

    #[test]
    fn no_suggestion_on_tie_or_when_known() {
        let tie = plan_with(&[(1, 10, Some(7)), (2, 10, Some(8))]);
        assert!(tie.project_suggestions(&BTreeMap::new(), &[]).is_empty());

        let plan = plan_with(&[(1, 10, Some(7))]);
        let known = BTreeMap::from([(10, ProjectLink { src_name: String::new(), dst_id: 9, dst_name: String::new() })]);
        assert!(plan.project_suggestions(&known, &[]).is_empty());
    }
}

/// Prueba de extremo a extremo contra los Redmine de `docker/test-redmines`.
/// `RDMN_A_URL=… RDMN_A_KEY=… RDMN_B_URL=… RDMN_B_KEY=… cargo test -- --ignored --nocapture`
#[cfg(test)]
mod live_tests {
    use super::*;
    use crate::app::dates;

    fn env(k: &str) -> String {
        std::env::var(k).unwrap_or_else(|_| panic!("falta {k}"))
    }

    fn plan(a: &Client, b: &Client, m: &PairMappings, ledger: &BTreeMap<u32, &LedgerEntry>) -> Plan {
        let (from, to) = dates::week_of(dates::today());
        let data = load(LoadRequest {
            src: a.clone(),
            dst: b.clone(),
            from: dates::iso(from),
            to: dates::iso(to),
            src_project: None,
            mappings: m.clone(),
        })
        .expect("carga");
        let user = b.current_user().unwrap();
        let projects = b.my_projects(&user).unwrap();
        Plan::build(
            data,
            PlanInput {
                src_id: "cliente",
                src_base: a.base_url(),
                dst_base: b.base_url(),
                dst_user_id: user.id,
                dst_projects: &projects,
                mappings: m,
                ledger,
                settings: &Settings::default(),
            },
        )
    }

    #[test]
    #[ignore]
    fn end_to_end() {
        let a = Client::new(&env("RDMN_A_URL"), &env("RDMN_A_KEY"), false);
        let b = Client::new(&env("RDMN_B_URL"), &env("RDMN_B_KEY"), false);
        let mut m = PairMappings::default();
        let empty = BTreeMap::new();
        let p = plan(&a, &b, &m, &empty);

        println!("== Imputaciones: {}", p.rows.len());
        for (id, r) in &p.issues {
            println!("issue #{id} {:?} -> {:?}", r.src.subject, r.state);
            for c in r.candidates.iter().take(3) {
                println!("    {:.2} #{} {} ({})", c.score, c.issue_id, c.subject, c.reason);
            }
        }
        for (name, r) in &p.activities {
            println!("actividad {name} -> {:?} confirmada={}", r.dst, r.confirmed);
        }
        assert_eq!(p.rows.len(), 6);
        assert!(p.activities.values().all(|a| !a.confirmed), "nombres distintos: requieren confirmación");
        assert_eq!(p.activities["Desarrollar"].dst.as_deref(), Some("Codificar"));
        assert_eq!(p.activities["Pruebas"].dst.as_deref(), Some("Testing"));
        assert_eq!(p.activities["Reunión"].dst.as_deref(), Some("Reuniones"));

        // Simular lo que confirmaría el usuario.
        assert_eq!(p.activities["Análisis"].dst, None, "sin parecido: no se sugiere nada");
        for (name, r) in &p.activities {
            m.set_activity(name, r.dst.as_deref().unwrap_or("Diseño"));
        }
        let dst_user = b.current_user().unwrap();
        let dst_project = b.my_projects(&dst_user).unwrap()[0].clone();
        for (id, r) in &p.issues {
            if let IssueState::Linked { dst_id, subject, project_id, .. } = &r.state {
                m.issues.insert(*id, crate::store::IssueLink {
                    src_subject: r.src.subject.clone(),
                    dst_id: *dst_id,
                    dst_subject: subject.clone(),
                    dst_project_id: *project_id,
                    origin: LinkOrigin::Auto,
                    synced: Default::default(),
                });
            }
        }
        let src_proj = p.rows.iter().find(|r| r.entry.issue.is_none()).unwrap().entry.project.clone();
        m.projects.insert(src_proj.id, crate::store::ProjectLink {
            src_name: src_proj.name,
            dst_id: dst_project.id,
            dst_name: dst_project.name,
        });

        let p = plan(&a, &b, &m, &empty);
        let items = p.build_items(true);
        println!("== Listas para enviar: {}", items.len());
        let mut created = Vec::new();
        execute(&b, items, |item, res| {
            println!("  #{} -> {:?}", item.src_entry_id, res.as_ref().map(|t| t.id));
            created.push(res.expect("creación"));
        });
        assert!(!created.is_empty());
        assert!(created.iter().all(|t| t.comments_text().contains("[sync:cliente#")));

        // Re-analizar: lo enviado debe figurar como ya sincronizado (por la marca).
        let p = plan(&a, &b, &m, &empty);
        let already = p.rows.iter().filter(|r| matches!(r.sync, RowSync::Already { .. })).count();
        println!("== Ya sincronizadas tras re-análisis: {already}");
        assert_eq!(already, created.len());
        assert!(p.build_items(true).len() + already <= p.rows.len());

        for t in created {
            b.delete_time_entry(t.id).unwrap();
        }

        // ---- Igualar el issue de DESTINO con el de origen (#1 "Implementar login con OAuth").
        use crate::issue_sync::{self, DiffCtx, Field, Pending};
        let IssueState::Linked { dst_id, .. } = p.issues[&1].state.clone() else { panic!("#1 sin enlazar") };
        let src_before = a.issue(1).unwrap();
        let dst_before = b.issue(dst_id).unwrap();
        let statuses = b.statuses().unwrap();
        let priorities = b.priorities().unwrap();
        let versions = HashMap::from([(dst_before.project.id, b.versions(dst_before.project.id).unwrap())]);
        let fields = crate::store::IssueFields::default();
        let diff = |m: &PairMappings| {
            let ctx = DiffCtx {
                pair: m,
                fields: &fields,
                respect_dst_changes: true,
                dst_statuses: &statuses,
                dst_priorities: &priorities,
                dst_versions: &versions,
            };
            issue_sync::diff(&src_before, &dst_before, None, &ctx)
        };
        let d = diff(&m);
        assert!(d.pending.contains(&Pending::Status("En curso".into())), "{:?}", d.pending);
        m.set_status("En curso", "En progreso");
        let d = diff(&m);
        let changed: Vec<Field> = d.changes.iter().map(|c| c.field).collect();
        println!("== Cambios en el destino: {:?}", d.changes.iter().map(|c| format!("{}: {} -> {}", c.field.label(), c.from, c.to)).collect::<Vec<_>>());
        for f in [Field::Status, Field::DoneRatio, Field::Version] {
            assert!(changed.contains(&f), "falta {f:?}");
        }

        let (after, ignored) = issue_sync::apply(&b, dst_id, &d.changes).expect("actualización");
        assert!(ignored.is_empty(), "ignorados: {ignored:?}");
        assert_eq!(after.status.name, "En progreso");
        assert_eq!(after.done_ratio, Some(50.0));
        assert_eq!(after.fixed_version.as_ref().map(|v| v.name.as_str()), Some("Sprint 23"));

        // El origen solo se ha leído.
        let src_after = a.issue(1).unwrap();
        assert_eq!(
            (src_after.status.id, src_after.done_ratio, src_after.fixed_version.map(|v| v.id), src_after.subject),
            (src_before.status.id, src_before.done_ratio, src_before.fixed_version.as_ref().map(|v| v.id), src_before.subject.clone())
        );

        // Dejar el destino como estaba.
        let mut restore = serde_json::Map::new();
        restore.insert("status_id".into(), dst_before.status.id.into());
        restore.insert("done_ratio".into(), (dst_before.done_ratio.unwrap_or(0.0) as u32).into());
        restore.insert("fixed_version_id".into(), serde_json::Value::Null);
        b.update_issue(dst_id, &restore).unwrap();
    }
}
