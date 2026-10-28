//! Encounters, observations and the problem list.

use chrono::NaiveDate;
use healthcraft_core::{Actor, ConditionId, EncounterId, ObservationId, PatientId, Timestamp};
use healthcraft_record::{ClinicalEvent, EncounterClass, ObservationValue};
use healthcraft_terminology::{CodeSystem, Coding};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{Access, CommandSpec, EngineError, Session, params};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "encounter.open",
            label: "Start encounter",
            params: r#"{"patientId":uuid,"class":"ambulatory|emergency|inpatient|virtual|home","reason"?:string,"expectedVersion"?:int}"#,
            access: Access::WriteChart,
            run: open,
        },
        CommandSpec {
            id: "encounter.close",
            label: "End encounter",
            params: r#"{"patientId":uuid,"encounterId":uuid,"expectedVersion"?:int}"#,
            access: Access::WriteChart,
            run: close,
        },
        CommandSpec {
            id: "observation.record",
            label: "Record observation",
            params: r#"{"patientId":uuid,"encounterId"?:uuid,"system":uri,"code":string,"display"?:string,"value":number|string,"unit"?:string,"effective"?:rfc3339,"expectedVersion"?:int}"#,
            access: Access::WriteChart,
            run: record_observation,
        },
        CommandSpec {
            id: "observation.enteredInError",
            label: "Mark observation entered in error",
            params: r#"{"patientId":uuid,"observationId":uuid,"reason":string,"expectedVersion"?:int}"#,
            access: Access::WriteChart,
            run: entered_in_error,
        },
        CommandSpec {
            id: "condition.record",
            label: "Add to problem list",
            params: r#"{"patientId":uuid,"system":uri,"code":string,"display"?:string,"onset"?:"YYYY-MM-DD","expectedVersion"?:int}"#,
            access: Access::WriteChart,
            run: record_condition,
        },
    ]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenParams {
    patient_id: PatientId,
    class: EncounterClass,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    expected_version: Option<u64>,
}

fn open(session: &Session, actor: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: OpenParams = params(value)?;
    let encounter = EncounterId::new(session.ids.as_ref());
    let event = ClinicalEvent::EncounterOpened { patient: p.patient_id, encounter, class: p.class, start: session.clock.now(), reason: p.reason };
    let version = session.append(actor, p.patient_id, p.expected_version, vec![event])?;
    Ok(json!({ "encounterId": encounter, "version": version }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CloseParams {
    patient_id: PatientId,
    encounter_id: EncounterId,
    #[serde(default)]
    expected_version: Option<u64>,
}

fn close(session: &Session, actor: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: CloseParams = params(value)?;
    let event = ClinicalEvent::EncounterClosed { patient: p.patient_id, encounter: p.encounter_id, end: session.clock.now() };
    let version = session.append(actor, p.patient_id, p.expected_version, vec![event])?;
    Ok(json!({ "version": version }))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawValue {
    Number(f64),
    Text(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ObservationParams {
    patient_id: PatientId,
    #[serde(default)]
    encounter_id: Option<EncounterId>,
    system: String,
    code: String,
    #[serde(default)]
    display: Option<String>,
    value: RawValue,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    effective: Option<Timestamp>,
    #[serde(default)]
    expected_version: Option<u64>,
}

fn coding(system: &str, code: String, display: Option<String>) -> Result<Coding, EngineError> {
    let system = CodeSystem::from_uri(system).map_err(|e| EngineError::BadParams(e.to_string()))?;
    Coding::new(system, code, display).map_err(|e| EngineError::BadParams(e.to_string()))
}

fn record_observation(session: &Session, actor: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: ObservationParams = params(value)?;
    let code = coding(&p.system, p.code, p.display)?;
    let value = match p.value {
        RawValue::Number(n) => ObservationValue::Quantity { value: n, unit: p.unit.unwrap_or_default() },
        RawValue::Text(t) => ObservationValue::Text(t),
    };
    let observation = ObservationId::new(session.ids.as_ref());
    let event = ClinicalEvent::ObservationRecorded {
        patient: p.patient_id,
        observation,
        encounter: p.encounter_id,
        code,
        value,
        effective: p.effective.unwrap_or_else(|| session.clock.now()),
    };
    let version = session.append(actor, p.patient_id, p.expected_version, vec![event])?;
    Ok(json!({ "observationId": observation, "version": version }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ErrorParams {
    patient_id: PatientId,
    observation_id: ObservationId,
    reason: String,
    #[serde(default)]
    expected_version: Option<u64>,
}

fn entered_in_error(session: &Session, actor: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: ErrorParams = params(value)?;
    let event = ClinicalEvent::ObservationEnteredInError { patient: p.patient_id, observation: p.observation_id, reason: p.reason };
    let version = session.append(actor, p.patient_id, p.expected_version, vec![event])?;
    Ok(json!({ "version": version }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConditionParams {
    patient_id: PatientId,
    system: String,
    code: String,
    #[serde(default)]
    display: Option<String>,
    #[serde(default)]
    onset: Option<NaiveDate>,
    #[serde(default)]
    expected_version: Option<u64>,
}

fn record_condition(session: &Session, actor: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: ConditionParams = params(value)?;
    let code = coding(&p.system, p.code, p.display)?;
    let condition = ConditionId::new(session.ids.as_ref());
    let event = ClinicalEvent::ConditionRecorded { patient: p.patient_id, condition, code, onset: p.onset };
    let version = session.append(actor, p.patient_id, p.expected_version, vec![event])?;
    Ok(json!({ "conditionId": condition, "version": version }))
}
