//! Interfaces: FHIR bundles (Synthea) and HL7 v2 lab results.

use chrono::{NaiveDate, NaiveDateTime};
use healthcraft_core::{Actor, ObservationId, PatientId, Timestamp};
use healthcraft_fhir::import::{Skipped, import_bundle};
use healthcraft_hl7v2::Message;
use healthcraft_hl7v2::oru::{LabValue, lab_results};
use healthcraft_record::{ClinicalEvent, ObservationValue};
use healthcraft_store::StoreError;
use healthcraft_terminology::{CodeSystem, Coding};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{Access, CommandSpec, EngineError, Session, params};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec { id: "import.fhirBundle", label: "Import FHIR bundle", params: r#"{"bundle":object|string}"#, access: Access::Import, run: fhir_bundle },
        CommandSpec {
            id: "import.hl7v2Oru",
            label: "Import HL7 v2 lab result",
            params: r#"{"patientId":uuid,"message":string}"#,
            access: Access::Import,
            run: hl7v2_oru,
        },
    ]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleParams {
    bundle: Value,
}

fn fhir_bundle(session: &Session, actor: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: BundleParams = params(value)?;
    let text = match p.bundle {
        Value::String(s) => s,
        other => other.to_string(),
    };
    let report = import_bundle(&text, session.ids.as_ref()).map_err(|e| EngineError::BadParams(e.to_string()))?;
    let mut skipped = report.skipped;
    let (mut patients, mut events) = (Vec::new(), 0usize);
    for imported in report.patients {
        let count = imported.events.len();
        match session.append(actor, imported.patient, Some(0), imported.events) {
            Ok(_) => {
                patients.push(imported.patient);
                events = events.saturating_add(count);
            }
            Err(EngineError::Store(StoreError::Conflict { .. })) => {
                skipped.push(Skipped { resource_type: "Patient".into(), id: Some(imported.patient.to_string()), reason: "already imported".into() })
            }
            Err(e) => skipped.push(Skipped { resource_type: "Patient".into(), id: Some(imported.patient.to_string()), reason: e.to_string() }),
        }
    }
    Ok(json!({ "patients": patients, "events": events, "skipped": skipped }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OruParams {
    patient_id: PatientId,
    message: String,
}

/// HL7 v2 timestamps: `YYYYMMDD[HHMM[SS]]`, optionally with fractional seconds or a UTC offset
/// (both ignored: results are placed on the sender's wall-clock time, as UTC).
fn hl7_time(ts: &str) -> Option<Timestamp> {
    let digits: String = ts.chars().take_while(char::is_ascii_digit).collect();
    match digits.len() {
        14 => NaiveDateTime::parse_from_str(&digits, "%Y%m%d%H%M%S").ok(),
        12 => NaiveDateTime::parse_from_str(&digits, "%Y%m%d%H%M").ok(),
        8 => NaiveDate::parse_from_str(&digits, "%Y%m%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)),
        _ => None,
    }
    .map(|t| t.and_utc())
}

fn hl7v2_oru(session: &Session, actor: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: OruParams = params(value)?;
    let msg = Message::parse(&p.message).map_err(|e| EngineError::BadParams(e.to_string()))?;
    let results = lab_results(&msg).map_err(|e| EngineError::BadParams(e.to_string()))?;
    // OBR-7 (observation time), else MSH-7 (message time), else now.
    let effective = msg
        .segment("OBR")
        .and_then(|obr| obr.component(7, 1))
        .or_else(|| msg.segment("MSH").and_then(|msh| msh.component(7, 1)))
        .and_then(hl7_time)
        .unwrap_or_else(|| session.clock.now());
    let mut events = Vec::with_capacity(results.len());
    for r in results {
        let system = match r.coding_system.as_deref() {
            Some("LN") => CodeSystem::Loinc,
            Some("SCT") => CodeSystem::SnomedCt,
            Some(other) => CodeSystem::Other(format!("urn:hl7v2:coding-system:{other}")),
            None => CodeSystem::Other("urn:hl7v2:coding-system:local".into()),
        };
        let code = Coding::new(system, r.code, r.display).map_err(|e| EngineError::BadParams(e.to_string()))?;
        let value = match r.value {
            LabValue::Numeric(v) => ObservationValue::Quantity { value: v, unit: r.units.unwrap_or_default() },
            LabValue::Text(t) => ObservationValue::Text(t),
        };
        events.push(ClinicalEvent::ObservationRecorded {
            patient: p.patient_id,
            observation: ObservationId::new(session.ids.as_ref()),
            encounter: None,
            code,
            value,
            effective,
        });
    }
    if events.is_empty() {
        return Err(EngineError::BadParams("message has no OBX results".into()));
    }
    let count = events.len();
    let version = session.append(actor, p.patient_id, None, events)?;
    Ok(json!({ "observations": count, "version": version, "controlId": msg.control_id() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hl7_timestamps() {
        assert_eq!(hl7_time("20261027120000").unwrap().to_rfc3339(), "2026-10-27T12:00:00+00:00");
        assert_eq!(hl7_time("202610271200-0500").unwrap().to_rfc3339(), "2026-10-27T12:00:00+00:00");
        assert_eq!(hl7_time("20261027").unwrap().to_rfc3339(), "2026-10-27T00:00:00+00:00");
        assert_eq!(hl7_time("2026"), None);
        assert_eq!(hl7_time("20261399"), None);
    }
}
