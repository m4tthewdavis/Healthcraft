//! Registering, finding and amending patients.

use chrono::NaiveDate;
use healthcraft_core::{Actor, PatientId};
use healthcraft_record::{Address, ClinicalEvent, Demographics, Sex};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{Access, CommandSpec, EngineError, Session, params, to_json};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "patient.register",
            label: "Register patient",
            params: r#"{"given":[string],"family":string,"birthDate":"YYYY-MM-DD","sex":"female|male|other|unknown","address"?:{...}}"#,
            access: Access::WriteChart,
            run: register,
        },
        CommandSpec {
            id: "patient.amendDemographics",
            label: "Amend demographics",
            params: r#"{"patientId":uuid,"expectedVersion"?:int,"given":[string],"family":string,"birthDate":"YYYY-MM-DD","sex":string,"address"?:{...},"reason":string}"#,
            access: Access::WriteChart,
            run: amend,
        },
        CommandSpec { id: "patient.list", label: "Patient list", params: "{}", access: Access::ReadChart, run: list },
        CommandSpec { id: "patient.chart", label: "Open chart", params: r#"{"patientId":uuid}"#, access: Access::ReadChart, run: chart },
    ]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DemographicsParams {
    #[serde(default)]
    given: Vec<String>,
    family: String,
    birth_date: NaiveDate,
    sex: Sex,
    #[serde(default)]
    address: Option<Address>,
}

impl From<DemographicsParams> for Demographics {
    fn from(p: DemographicsParams) -> Self {
        Self { given: p.given, family: p.family, birth_date: p.birth_date, sex: p.sex, address: p.address }
    }
}

fn register(session: &Session, actor: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: DemographicsParams = params(value)?;
    let patient = PatientId::new(session.ids.as_ref());
    let version = session.append(actor, patient, Some(0), vec![ClinicalEvent::PatientRegistered { patient, demographics: p.into() }])?;
    Ok(json!({ "patientId": patient, "version": version }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AmendParams {
    patient_id: PatientId,
    #[serde(default)]
    expected_version: Option<u64>,
    reason: String,
    #[serde(default)]
    given: Vec<String>,
    family: String,
    birth_date: NaiveDate,
    sex: Sex,
    #[serde(default)]
    address: Option<Address>,
}

fn amend(session: &Session, actor: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: AmendParams = params(value)?;
    let demographics = Demographics { given: p.given, family: p.family, birth_date: p.birth_date, sex: p.sex, address: p.address };
    let event = ClinicalEvent::DemographicsAmended { patient: p.patient_id, demographics, reason: p.reason };
    let version = session.append(actor, p.patient_id, p.expected_version, vec![event])?;
    Ok(json!({ "patientId": p.patient_id, "version": version }))
}

fn list(session: &Session, _: &Actor, _: &Value) -> Result<Value, EngineError> {
    let mut rows = Vec::new();
    for patient in session.store.patients() {
        let chart = session.chart(patient)?;
        let d = &chart.demographics;
        rows.push(json!({
            "patientId": patient,
            "name": d.display_name(),
            "family": d.family,
            "birthDate": d.birth_date,
            "sex": d.sex,
            "version": chart.version,
        }));
    }
    rows.sort_by(|a, b| a["family"].as_str().cmp(&b["family"].as_str()).then_with(|| a["name"].as_str().cmp(&b["name"].as_str())));
    Ok(Value::Array(rows))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PatientParams {
    pub patient_id: PatientId,
}

fn chart(session: &Session, _: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: PatientParams = params(value)?;
    to_json(&session.chart(p.patient_id)?)
}
