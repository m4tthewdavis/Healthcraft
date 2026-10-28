//! The engine: a [`Session`] and the command registry.
//!
//! **Everything is a command.** Each action has a stable id (`patient.register`,
//! `observation.record`, `research.deidentify`), JSON parameters and a JSON result. The desktop
//! UI, the HTTP API, the CLI and (later) an MCP server for agents all call [`Session::dispatch`];
//! none of them touch the store directly.
//!
//! Dispatch does three things for every call, whoever makes it:
//!
//! 1. **authorizes** it: the actor's [`Role`] must allow the command's [`Access`];
//! 2. **runs** it: commands validate their parameters and return [`EngineError`], never panic;
//! 3. **audits** it: one [`healthcraft_audit`] entry per call, including refusals and failures.

use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use healthcraft_audit::{Action, AuditLog, NewEntry, Outcome};
use healthcraft_core::{Actor, Clock, IdSource, PatientId, Role};
use healthcraft_record::{Chart, ClinicalEvent};
use healthcraft_research::deid::Pseudonymizer;
use healthcraft_store::{EventStore, StoreError};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

mod admin_cmds;
mod clinical_cmds;
pub mod demo;
mod fhir_cmds;
mod import_cmds;
mod patient_cmds;
mod research_cmds;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("role {role:?} may not run `{command}`")]
    Denied { command: String, role: Role },
    #[error("invalid parameters: {0}")]
    BadParams(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Failed(String),
}

/// What a command touches, which decides who may run it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    /// Listing commands and other metadata.
    Meta,
    /// Reading identified patient data.
    ReadChart,
    /// Changing a chart.
    WriteChart,
    /// Bringing data in through an interface.
    Import,
    /// Identified bulk export (OMOP tables keep full dates).
    Export,
    /// De-identified data only.
    Research,
    /// The access log.
    Audit,
}

impl Access {
    fn audit_action(self) -> Action {
        match self {
            Self::Meta | Self::ReadChart | Self::Audit => Action::Read,
            Self::WriteChart => Action::Write,
            Self::Import => Action::Import,
            Self::Export => Action::Export,
            Self::Research => Action::Deidentify,
        }
    }
}

/// The role-based access policy. Deliberately small and in one place; per-patient consent and
/// care-team scoping are on the roadmap.
pub fn allowed(role: Role, access: Access) -> bool {
    matches!(
        (role, access),
        (_, Access::Meta)
            | (Role::System, _)
            | (Role::Clinician, Access::ReadChart | Access::WriteChart)
            | (Role::Researcher, Access::Research)
            | (Role::Admin, Access::ReadChart | Access::Import | Access::Export | Access::Research | Access::Audit)
    )
}

pub type RunFn = fn(&Session, &Actor, &Value) -> Result<Value, EngineError>;

/// One command: what agents and UIs read through `commands.list`, and the function that runs it.
#[derive(Clone, Copy)]
pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// Parameter summary, e.g. `{"patientId":uuid,"reason"?:string}`.
    pub params: &'static str,
    pub access: Access,
    pub run: RunFn,
}

/// Every registered command.
pub fn registry() -> &'static [CommandSpec] {
    static REGISTRY: OnceLock<Vec<CommandSpec>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut v = Vec::new();
        v.extend(admin_cmds::specs());
        v.extend(patient_cmds::specs());
        v.extend(clinical_cmds::specs());
        v.extend(fhir_cmds::specs());
        v.extend(import_cmds::specs());
        v.extend(research_cmds::specs());
        v
    })
}

pub struct Session {
    store: Arc<dyn EventStore>,
    audit: Arc<AuditLog>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdSource>,
    pseudonyms: Mutex<Pseudonymizer>,
}

impl Session {
    pub fn new(store: Arc<dyn EventStore>, clock: Arc<dyn Clock>, ids: Arc<dyn IdSource>) -> Self {
        Self { store, audit: Arc::new(AuditLog::default()), clock, ids, pseudonyms: Mutex::new(Pseudonymizer::default()) }
    }

    /// An in-memory session on the wall clock with random ids (desktop demo, CLI, development
    /// server).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn in_memory() -> Self {
        Self::new(Arc::new(healthcraft_store::InMemoryStore::default()), Arc::new(healthcraft_core::SystemClock), Arc::new(healthcraft_core::RandomIds))
    }

    /// The session's current time (what new events will be stamped with).
    pub fn now(&self) -> healthcraft_core::Timestamp {
        self.clock.now()
    }

    pub fn audit_log(&self) -> &AuditLog {
        &self.audit
    }

    /// Authorizes, runs and audits one command.
    pub fn dispatch(&self, actor: &Actor, id: &str, params: &Value) -> Result<Value, EngineError> {
        let resource = resource_of(params);
        let Some(spec) = registry().iter().find(|s| s.id == id) else {
            self.record(actor, Action::Read, id, resource, Outcome::Failed);
            return Err(EngineError::UnknownCommand(id.chars().take(128).collect()));
        };
        let action = spec.access.audit_action();
        if !allowed(actor.role, spec.access) {
            self.record(actor, action, spec.id, resource, Outcome::Denied);
            return Err(EngineError::Denied { command: spec.id.into(), role: actor.role });
        }
        let result = (spec.run)(self, actor, params);
        self.record(actor, action, spec.id, resource, if result.is_ok() { Outcome::Success } else { Outcome::Failed });
        result
    }

    fn record(&self, actor: &Actor, action: Action, command: &str, resource: String, outcome: Outcome) {
        self.audit.append(NewEntry { at: self.clock.now(), actor: actor.clone(), action, command: command.chars().take(128).collect(), resource, outcome });
    }

    fn chart(&self, patient: PatientId) -> Result<Chart, EngineError> {
        Ok(self.store.chart(patient)?)
    }

    /// Appends events to a record. Without `expected_version` the current version is read first,
    /// so a concurrent writer still causes a conflict rather than an overwrite.
    fn append(&self, actor: &Actor, patient: PatientId, expected_version: Option<u64>, events: Vec<ClinicalEvent>) -> Result<u64, EngineError> {
        let expected = match expected_version {
            Some(v) => v,
            None => match self.store.read(patient) {
                Ok(events) => u64::try_from(events.len()).unwrap_or(u64::MAX),
                Err(StoreError::NotFound(_)) => 0,
                Err(e) => return Err(e.into()),
            },
        };
        let stored = self.store.append(patient, expected, actor, self.clock.now(), self.ids.as_ref(), events)?;
        Ok(stored.last().map_or(expected, |e| e.sequence))
    }

    fn pseudonyms(&self) -> std::sync::MutexGuard<'_, Pseudonymizer> {
        self.pseudonyms.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The audit resource for a call: `Patient/<id>` when the parameters name one patient, else `*`.
fn resource_of(params: &Value) -> String {
    match params.get("patientId").and_then(Value::as_str).and_then(|s| s.parse::<PatientId>().ok()) {
        Some(p) => format!("Patient/{p}"),
        None => "*".into(),
    }
}

/// Deserializes command parameters; `null` counts as `{}`.
fn params<T: DeserializeOwned>(value: &Value) -> Result<T, EngineError> {
    let value = if value.is_null() { Value::Object(Default::default()) } else { value.clone() };
    serde_json::from_value(value).map_err(|e| EngineError::BadParams(e.to_string()))
}

fn to_json<T: Serialize>(value: &T) -> Result<Value, EngineError> {
    serde_json::to_value(value).map_err(|e| EngineError::Failed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_ids_are_unique_and_well_formed() {
        let mut ids: Vec<&str> = registry().iter().map(|s| s.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate command id");
        for id in ids {
            assert!(id.contains('.') && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.'), "{id}");
        }
    }

    #[test]
    fn policy() {
        assert!(allowed(Role::Clinician, Access::WriteChart));
        assert!(!allowed(Role::Clinician, Access::Export));
        assert!(!allowed(Role::Researcher, Access::ReadChart));
        assert!(allowed(Role::Researcher, Access::Research));
        assert!(!allowed(Role::Admin, Access::WriteChart));
        assert!(allowed(Role::System, Access::Audit));
    }
}
