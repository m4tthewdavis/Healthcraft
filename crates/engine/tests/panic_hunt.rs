//! Every command, adversarial parameters: each call must return `Ok` or `Err`, never panic
//! (AGENTS.md › Never crash). Runs on every `cargo test`.

use healthcraft_core::Role;
use healthcraft_engine::registry;
use healthcraft_testkit::{SYNTHETIC_BUNDLE, SYNTHETIC_PATIENT_ID, actor, session};
use serde_json::{Value, json};

fn hostile_values() -> Vec<Value> {
    vec![
        Value::Null,
        json!(true),
        json!(-1),
        json!(1e308),
        json!(u64::MAX),
        json!(""),
        json!("\u{0}"),
        json!("x".repeat(100_000)),
        json!([]),
        json!({}),
        json!(SYNTHETIC_PATIENT_ID),
        json!("00000000-0000-0000-0000-000000000000"),
        json!("9999-99-99"),
        json!({"nested": {"deep": [1, 2, 3]}}),
    ]
}

const FIELDS: &[&str] = &[
    "patientId",
    "encounterId",
    "observationId",
    "expectedVersion",
    "given",
    "family",
    "birthDate",
    "sex",
    "address",
    "reason",
    "class",
    "system",
    "code",
    "display",
    "value",
    "unit",
    "effective",
    "onset",
    "bundle",
    "message",
    "asOf",
    "start",
    "end",
    "title",
    "text",
    "signed",
];

#[test]
fn no_command_panics_on_hostile_params() {
    let s = session();
    let system = actor(Role::System);
    s.dispatch(&system, "import.fhirBundle", &json!({ "bundle": SYNTHETIC_BUNDLE })).unwrap();
    let mut calls = 0usize;
    for spec in registry() {
        for top in hostile_values() {
            let _ = s.dispatch(&system, spec.id, &top);
            calls += 1;
        }
        for field in FIELDS {
            for value in hostile_values() {
                let mut params = json!({ "patientId": SYNTHETIC_PATIENT_ID });
                params[*field] = value;
                let _ = s.dispatch(&system, spec.id, &params);
                calls += 1;
            }
        }
    }
    assert!(calls > 1000);
    // The audit chain survives all of it.
    assert!(s.dispatch(&system, "audit.verify", &Value::Null).is_ok());
}
