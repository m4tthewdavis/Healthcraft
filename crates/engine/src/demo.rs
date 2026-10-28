//! A small synthetic cohort for demos (the desktop app and the browser build), written through the
//! same commands an interface would use. Every person here is invented; the numbered names follow
//! Synthea's convention so nobody mistakes them for real patients.

use healthcraft_core::Actor;
use serde_json::{Value, json};

use crate::{EngineError, Session};

const LOINC: &str = "http://loinc.org";
const SNOMED: &str = "http://snomed.info/sct";

struct Person {
    given: &'static str,
    family: &'static str,
    birth_date: &'static str,
    sex: &'static str,
    city: &'static str,
    state: &'static str,
    zip: &'static str,
    /// (SNOMED CT code, display, onset)
    conditions: &'static [(&'static str, &'static str, &'static str)],
    /// (LOINC code, display, value, unit, effective)
    results: &'static [(&'static str, &'static str, f64, &'static str, &'static str)],
}

const A1C: (&str, &str) = ("4548-4", "Hemoglobin A1c/Hemoglobin.total in Blood");
const BMI: (&str, &str) = ("39156-5", "Body mass index (BMI) [Ratio]");
const SBP: (&str, &str) = ("8480-6", "Systolic blood pressure");
const LDL: (&str, &str) = ("2089-1", "Cholesterol in LDL [Mass/volume] in Serum or Plasma");

const COHORT: &[Person] = &[
    Person {
        given: "Marisol312",
        family: "Quintero845",
        birth_date: "1958-09-14",
        sex: "female",
        city: "Worcester",
        state: "MA",
        zip: "01608",
        conditions: &[("44054006", "Diabetes mellitus type 2", "2016-03-02"), ("59621000", "Essential hypertension", "2012-07-19")],
        results: &[
            (A1C.0, A1C.1, 7.9, "%", "2025-02-11T09:20:00Z"),
            (A1C.0, A1C.1, 7.1, "%", "2025-08-19T10:05:00Z"),
            (SBP.0, SBP.1, 142.0, "mm[Hg]", "2025-08-19T09:50:00Z"),
            (BMI.0, BMI.1, 33.2, "kg/m2", "2025-08-19T09:50:00Z"),
        ],
    },
    Person {
        given: "Desmond87",
        family: "Okafor611",
        birth_date: "1971-01-30",
        sex: "male",
        city: "Springfield",
        state: "MA",
        zip: "01103",
        conditions: &[("15777000", "Prediabetes", "2023-05-10"), ("55822004", "Hyperlipidemia", "2023-05-10")],
        results: &[
            (A1C.0, A1C.1, 6.1, "%", "2025-05-02T08:40:00Z"),
            (LDL.0, LDL.1, 148.0, "mg/dL", "2025-05-02T08:40:00Z"),
            (BMI.0, BMI.1, 29.4, "kg/m2", "2025-05-02T08:30:00Z"),
        ],
    },
    Person {
        given: "Hazel908",
        family: "Lindqvist52",
        birth_date: "1934-06-02",
        sex: "female",
        city: "Concord",
        state: "NH",
        zip: "03301",
        conditions: &[("49436004", "Atrial fibrillation", "2019-11-05"), ("59621000", "Essential hypertension", "2004-03-18")],
        results: &[(SBP.0, SBP.1, 151.0, "mm[Hg]", "2025-07-08T14:15:00Z")],
    },
    Person {
        given: "Tomasz71",
        family: "Wierzbicki20",
        birth_date: "1989-12-12",
        sex: "male",
        city: "Acworth",
        state: "NH",
        zip: "03601",
        conditions: &[("195967001", "Asthma", "2001-04-22")],
        results: &[(BMI.0, BMI.1, 24.1, "kg/m2", "2025-03-27T16:00:00Z")],
    },
    Person {
        given: "Priya433",
        family: "Raman960",
        birth_date: "1983-03-27",
        sex: "female",
        city: "Lowell",
        state: "MA",
        zip: "01852",
        conditions: &[("44054006", "Diabetes mellitus type 2", "2021-09-30"), ("162864005", "Body mass index 30+ - obesity", "2021-09-30")],
        results: &[
            (A1C.0, A1C.1, 8.6, "%", "2025-01-14T11:00:00Z"),
            (A1C.0, A1C.1, 7.8, "%", "2025-07-22T11:10:00Z"),
            (BMI.0, BMI.1, 36.0, "kg/m2", "2025-07-22T10:55:00Z"),
        ],
    },
    Person {
        given: "Elijah55",
        family: "Brooks388",
        birth_date: "1946-10-08",
        sex: "male",
        city: "Pittsfield",
        state: "MA",
        zip: "01201",
        conditions: &[("13645005", "Chronic obstructive lung disease", "2015-02-14"), ("55822004", "Hyperlipidemia", "2010-06-01")],
        results: &[(LDL.0, LDL.1, 121.0, "mg/dL", "2025-06-30T09:00:00Z"), (SBP.0, SBP.1, 136.0, "mm[Hg]", "2025-06-30T08:45:00Z")],
    },
];

fn id(value: &Value, key: &str) -> Result<Value, EngineError> {
    value.get(key).cloned().ok_or_else(|| EngineError::Failed(format!("demo: reply has no {key}")))
}

/// Registers the demo cohort and returns how many patients it added.
pub fn seed(session: &Session) -> Result<usize, EngineError> {
    let system = Actor::system();
    for p in COHORT {
        let reply = session.dispatch(
            &system,
            "patient.register",
            &json!({
                "given": [p.given], "family": p.family, "birthDate": p.birth_date, "sex": p.sex,
                "address": { "city": p.city, "state": p.state, "postalCode": p.zip, "country": "US" }
            }),
        )?;
        let patient = id(&reply, "patientId")?;
        for (code, display, onset) in p.conditions {
            session.dispatch(
                &system,
                "condition.record",
                &json!({ "patientId": patient, "system": SNOMED, "code": code, "display": display, "onset": onset }),
            )?;
        }
        for (code, display, value, unit, effective) in p.results {
            session.dispatch(
                &system,
                "observation.record",
                &json!({ "patientId": patient, "system": LOINC, "code": code, "display": display, "value": value, "unit": unit, "effective": effective }),
            )?;
        }
    }
    Ok(COHORT.len())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use healthcraft_core::{FixedClock, Role, SequentialIds, Timestamp};
    use healthcraft_store::InMemoryStore;

    use super::*;

    #[test]
    fn seeds_a_valid_cohort() {
        let at = Timestamp::from_timestamp(1_793_102_400, 0).unwrap();
        let s = Session::new(Arc::new(InMemoryStore::default()), Arc::new(FixedClock::new(at)), Arc::new(SequentialIds::default()));
        assert_eq!(seed(&s).unwrap(), COHORT.len());
        let researcher = Actor::new("r", Role::Researcher).unwrap();
        let rows = s.dispatch(&researcher, "research.deidentify", &json!({ "asOf": "2026-10-27" })).unwrap();
        let rows = rows.as_array().unwrap();
        assert_eq!(rows.len(), COHORT.len());
        assert!(rows.iter().any(|r| r["age90OrOver"] == true), "Hazel908 is 92");
        assert!(rows.iter().any(|r| r["zip3"] == "000"), "036xx is a restricted ZIP3");
    }
}
