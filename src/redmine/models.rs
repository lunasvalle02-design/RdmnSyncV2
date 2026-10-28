//! Modelos de la API REST de Redmine (solo los campos que usa la aplicación).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct IdName {
    pub id: u32,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IdOnly {
    pub id: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: u32,
    #[serde(default)]
    pub login: String,
    #[serde(default)]
    pub firstname: String,
    #[serde(default)]
    pub lastname: String,
    #[serde(default)]
    pub memberships: Vec<Membership>,
}

impl User {
    pub fn display_name(&self) -> String {
        let full = format!("{} {}", self.firstname, self.lastname);
        let full = full.trim();
        if full.is_empty() { self.login.clone() } else { full.to_string() }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Membership {
    pub project: IdName,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub parent: Option<IdName>,
    #[serde(default)]
    pub time_entry_activities: Vec<Activity>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Activity {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tracker {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Priority {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomFieldValue {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub value: Value,
}

impl CustomFieldValue {
    pub fn value_text(&self) -> String {
        match &self.value {
            Value::Null => String::new(),
            Value::String(s) => s.clone(),
            Value::Array(items) => items
                .iter()
                .map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))
                .collect::<Vec<_>>()
                .join(", "),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Issue {
    pub id: u32,
    pub project: IdName,
    pub tracker: IdName,
    pub status: IdName,
    pub priority: IdName,
    #[serde(default)]
    pub author: Option<IdName>,
    #[serde(default)]
    pub assigned_to: Option<IdName>,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub done_ratio: Option<f32>,
    #[serde(default)]
    pub estimated_hours: Option<f64>,
    #[serde(default)]
    pub spent_hours: Option<f64>,
    #[serde(default)]
    pub custom_fields: Vec<CustomFieldValue>,
}

impl Issue {
    pub fn description_text(&self) -> &str {
        self.description.as_deref().unwrap_or("")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TimeEntry {
    pub id: u32,
    pub project: IdName,
    #[serde(default)]
    pub issue: Option<IdOnly>,
    pub user: IdName,
    pub activity: IdName,
    pub hours: f64,
    #[serde(default)]
    pub comments: Option<String>,
    pub spent_on: String,
}

impl TimeEntry {
    pub fn comments_text(&self) -> &str {
        self.comments.as_deref().unwrap_or("")
    }

    pub fn issue_id(&self) -> Option<u32> {
        self.issue.as_ref().map(|i| i.id)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NewTimeEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<u32>,
    pub spent_on: String,
    pub hours: f64,
    pub activity_id: u32,
    pub comments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CustomFieldInput {
    pub id: u32,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewIssue {
    pub project_id: u32,
    pub tracker_id: u32,
    pub subject: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_hours: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_ratio: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub custom_fields: Vec<CustomFieldInput>,
}
