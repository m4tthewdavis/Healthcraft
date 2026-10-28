//! The chart as FHIR R4 resources (what the HTTP API's `/fhir/r4/*` routes return).

use healthcraft_core::Actor;
use healthcraft_fhir::export;
use healthcraft_fhir::resources::Resource;
use serde_json::Value;

use crate::patient_cmds::PatientParams;
use crate::{Access, CommandSpec, EngineError, Session, params, to_json};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec { id: "fhir.patient.read", label: "FHIR Patient", params: r#"{"patientId":uuid}"#, access: Access::ReadChart, run: read },
        CommandSpec {
            id: "fhir.patient.everything",
            label: "FHIR Patient $everything",
            params: r#"{"patientId":uuid}"#,
            access: Access::ReadChart,
            run: everything,
        },
        CommandSpec { id: "fhir.patient.search", label: "FHIR Patient search", params: "{}", access: Access::ReadChart, run: search },
    ]
}

fn read(session: &Session, _: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: PatientParams = params(value)?;
    to_json(&Resource::Patient(export::patient(&session.chart(p.patient_id)?)))
}

fn everything(session: &Session, _: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: PatientParams = params(value)?;
    to_json(&Resource::Bundle(export::everything(&session.chart(p.patient_id)?)))
}

fn search(session: &Session, _: &Actor, _: &Value) -> Result<Value, EngineError> {
    let mut patients = Vec::new();
    for id in session.store.patients() {
        patients.push(export::patient(&session.chart(id)?));
    }
    to_json(&Resource::Bundle(export::searchset(patients)))
}
