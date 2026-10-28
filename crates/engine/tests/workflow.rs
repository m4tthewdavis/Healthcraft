//! End-to-end through the engine: the same calls the UI, HTTP API and CLI make.

use healthcraft_core::Role;
use healthcraft_engine::EngineError;
use healthcraft_testkit::{SYNTHETIC_BUNDLE, SYNTHETIC_ORU, SYNTHETIC_PATIENT_ID, actor, session};
use serde_json::{Value, json};

#[test]
fn clinician_charts_a_visit() {
    let s = session();
    let dr = actor(Role::Clinician);
    let reg = s.dispatch(&dr, "patient.register", &json!({"given":["Synthetic"],"family":"Visitor","birthDate":"1990-02-03","sex":"male"})).unwrap();
    let patient = reg["patientId"].as_str().unwrap().to_owned();
    let enc = s.dispatch(&dr, "encounter.open", &json!({"patientId": patient, "class": "ambulatory", "reason": "Follow-up"})).unwrap();
    let encounter = enc["encounterId"].as_str().unwrap().to_owned();
    let obs = s
        .dispatch(
            &dr,
            "observation.record",
            &json!({"patientId": patient, "encounterId": encounter, "system": "http://loinc.org", "code": "8867-4", "display": "Heart rate", "value": 72, "unit": "/min"}),
        )
        .unwrap();
    s.dispatch(&dr, "encounter.close", &json!({"patientId": patient, "encounterId": encounter})).unwrap();
    s.dispatch(&dr, "observation.enteredInError", &json!({"patientId": patient, "observationId": obs["observationId"], "reason": "wrong device"})).unwrap();

    let chart = s.dispatch(&dr, "patient.chart", &json!({"patientId": patient})).unwrap();
    assert_eq!(chart["version"], 5);
    assert_eq!(chart["observations"][0]["enteredInError"], "wrong device");
    let fhir = s.dispatch(&dr, "fhir.patient.everything", &json!({"patientId": patient})).unwrap();
    assert_eq!(fhir["entry"][2]["resource"]["status"], "entered-in-error");

    // Recording into the closed encounter is refused by the record.
    let err = s
        .dispatch(
            &dr,
            "observation.record",
            &json!({"patientId": patient, "encounterId": encounter, "system": "http://loinc.org", "code": "8867-4", "value": 70}),
        )
        .unwrap_err();
    assert!(matches!(err, EngineError::Store(_)), "{err}");
}

#[test]
fn stale_writers_get_a_conflict() {
    let s = session();
    let dr = actor(Role::Clinician);
    let reg = s.dispatch(&dr, "patient.register", &json!({"family":"Conflict","birthDate":"1990-02-03","sex":"unknown"})).unwrap();
    let patient = reg["patientId"].clone();
    s.dispatch(&dr, "encounter.open", &json!({"patientId": patient, "class": "virtual", "expectedVersion": 1})).unwrap();
    let err = s.dispatch(&dr, "encounter.open", &json!({"patientId": patient, "class": "virtual", "expectedVersion": 1})).unwrap_err();
    assert!(err.to_string().contains("re-read and retry"), "{err}");
}

#[test]
fn imports_synthea_and_hl7_then_serves_research() {
    let s = session();
    let system = actor(Role::System);
    let report = s.dispatch(&system, "import.fhirBundle", &json!({ "bundle": SYNTHETIC_BUNDLE })).unwrap();
    assert_eq!(report["patients"], json!([SYNTHETIC_PATIENT_ID]));
    assert_eq!(report["skipped"].as_array().unwrap().len(), 2);
    let again = s.dispatch(&system, "import.fhirBundle", &json!({ "bundle": SYNTHETIC_BUNDLE })).unwrap();
    assert_eq!(again["patients"], json!([]));
    assert!(again["skipped"].as_array().unwrap().iter().any(|s| s["reason"] == "already imported"));

    let lab = s.dispatch(&system, "import.hl7v2Oru", &json!({ "patientId": SYNTHETIC_PATIENT_ID, "message": SYNTHETIC_ORU })).unwrap();
    assert_eq!(lab["observations"], 2);
    assert_eq!(lab["controlId"], "MSG00001");

    let omop = s.dispatch(&actor(Role::Admin), "research.omop.export", &Value::Null).unwrap();
    assert_eq!(omop["person"][0]["gender_concept_id"], 8532);
    assert_eq!(omop["measurement"].as_array().unwrap().len(), 5);

    let deid = s.dispatch(&actor(Role::Researcher), "research.deidentify", &json!({"asOf": "2026-10-27"})).unwrap();
    let text = deid.to_string();
    assert_eq!(deid[0]["birthYear"], 1961);
    for identifier in ["Testpatient", "Springfield", "1 Example Way", SYNTHETIC_PATIENT_ID] {
        assert!(!text.contains(identifier), "{identifier} leaked");
    }
}

#[test]
fn roles_are_enforced_and_every_call_is_audited() {
    let s = session();
    let system = actor(Role::System);
    s.dispatch(&system, "import.fhirBundle", &json!({ "bundle": SYNTHETIC_BUNDLE })).unwrap();
    let researcher = actor(Role::Researcher);
    let chart = s.dispatch(&researcher, "patient.chart", &json!({"patientId": SYNTHETIC_PATIENT_ID}));
    assert!(matches!(chart, Err(EngineError::Denied { .. })));
    let admin = actor(Role::Admin);
    assert!(matches!(s.dispatch(&admin, "patient.register", &json!({})), Err(EngineError::Denied { .. })));
    assert!(matches!(s.dispatch(&actor(Role::Clinician), "nope", &Value::Null), Err(EngineError::UnknownCommand(_))));

    let log = s.dispatch(&admin, "audit.entries", &Value::Null).unwrap();
    let outcomes: Vec<&str> = log.as_array().unwrap().iter().map(|e| e["outcome"].as_str().unwrap()).collect();
    assert_eq!(outcomes, ["success", "denied", "denied", "failed"]);
    assert_eq!(log[1]["resource"], format!("Patient/{SYNTHETIC_PATIENT_ID}"));
    assert_eq!(log[1]["actor"]["id"], "researcher.test");
    let verified = s.dispatch(&admin, "audit.verify", &Value::Null).unwrap();
    assert_eq!(verified["entries"], 5, "the audit.entries call itself is logged too");
}

#[test]
fn commands_list_reports_permissions() {
    let s = session();
    let list = s.dispatch(&actor(Role::Researcher), "commands.list", &Value::Null).unwrap();
    let allowed: Vec<&str> = list.as_array().unwrap().iter().filter(|c| c["allowed"] == true).map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(allowed, ["commands.list", "research.deidentify"]);
}
