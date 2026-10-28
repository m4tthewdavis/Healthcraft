//! A small synthetic cohort for demos (the desktop app and the browser build), written through the
//! same commands an interface would use. Every person and note here is invented; the numbered
//! names follow Synthea's convention so nobody mistakes them for real patients.

use healthcraft_core::Actor;
use serde_json::{Value, json};

use crate::{EngineError, Session};

const LOINC: &str = "http://loinc.org";
const SNOMED: &str = "http://snomed.info/sct";

type LabResult = (&'static str, &'static str, f64, &'static str);

const A1C: (&str, &str) = ("4548-4", "Hemoglobin A1c/Hemoglobin.total in Blood");
const BMI: (&str, &str) = ("39156-5", "Body mass index (BMI) [Ratio]");
const SBP: (&str, &str) = ("8480-6", "Systolic blood pressure");
const LDL: (&str, &str) = ("2089-1", "Cholesterol in LDL [Mass/volume] in Serum or Plasma");

const fn r(test: (&'static str, &'static str), value: f64, unit: &'static str) -> LabResult {
    (test.0, test.1, value, unit)
}

struct Visit {
    start: &'static str,
    end: &'static str,
    class: &'static str,
    reason: &'static str,
    results: &'static [LabResult],
    note: (&'static str, &'static str),
}

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
    visits: &'static [Visit],
}

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
        visits: &[
            Visit {
                start: "2025-02-11T09:00:00Z",
                end: "2025-02-11T09:40:00Z",
                class: "ambulatory",
                reason: "Diabetes follow-up",
                results: &[r(A1C, 7.9, "%"), r(SBP, 148.0, "mm[Hg]")],
                note: (
                    "Progress note",
                    "S: Reports missing evening metformin doses about twice a week. No hypoglycemia.\nO: BP 148/88. A1c 7.9%.\nA: Type 2 diabetes, above goal. Hypertension, above goal.\nP: Pill organizer and phone reminder. Recheck A1c in 6 months. Home BP log.",
                ),
            },
            Visit {
                start: "2025-08-19T09:30:00Z",
                end: "2025-08-19T10:10:00Z",
                class: "ambulatory",
                reason: "Diabetes follow-up",
                results: &[r(A1C, 7.1, "%"), r(SBP, 142.0, "mm[Hg]"), r(BMI, 33.2, "kg/m2")],
                note: (
                    "Progress note",
                    "S: Adherence improved with the organizer. Walking 20 minutes most days.\nO: BP 142/84. A1c 7.1% (from 7.9%).\nA: Type 2 diabetes, improving.\nP: Continue current regimen. Dietitian referral. Recheck in 6 months.",
                ),
            },
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
        visits: &[Visit {
            start: "2025-05-02T08:15:00Z",
            end: "2025-05-02T08:50:00Z",
            class: "ambulatory",
            reason: "Annual physical",
            results: &[r(A1C, 6.1, "%"), r(LDL, 148.0, "mg/dL"), r(BMI, 29.4, "kg/m2")],
            note: (
                "Annual physical",
                "S: No complaints. Desk job, little exercise.\nO: BMI 29.4. A1c 6.1%. LDL 148 mg/dL.\nA: Prediabetes. Hyperlipidemia.\nP: Lifestyle program referral. Discussed statin; patient prefers a 3-month lifestyle trial first. Repeat lipids in 3 months.",
            ),
        }],
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
        visits: &[Visit {
            start: "2025-07-08T14:00:00Z",
            end: "2025-07-08T14:30:00Z",
            class: "virtual",
            reason: "Anticoagulation check-in",
            results: &[r(SBP, 151.0, "mm[Hg]")],
            note: (
                "Telehealth note",
                "S: Daughter present on video. No bleeding, no falls since last visit.\nO: Home BP 151/82.\nA: Atrial fibrillation on anticoagulation, stable. Hypertension, above goal.\nP: Continue anticoagulant. Nurse to review home BP log in 2 weeks.",
            ),
        }],
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
        visits: &[Visit {
            start: "2025-03-27T15:45:00Z",
            end: "2025-03-27T16:15:00Z",
            class: "ambulatory",
            reason: "Asthma review",
            results: &[r(BMI, 24.1, "kg/m2")],
            note: (
                "Progress note",
                "S: Uses rescue inhaler about once a week, more in spring. No night symptoms.\nO: Lungs clear. BMI 24.1.\nA: Asthma, well controlled.\nP: Continue current inhalers. Written action plan reviewed.",
            ),
        }],
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
        visits: &[
            Visit {
                start: "2025-01-14T10:45:00Z",
                end: "2025-01-14T11:20:00Z",
                class: "ambulatory",
                reason: "Diabetes follow-up",
                results: &[r(A1C, 8.6, "%"), r(BMI, 37.1, "kg/m2")],
                note: (
                    "Progress note",
                    "S: Busy schedule, frequent takeout. Interested in a weight-management program.\nO: A1c 8.6%. BMI 37.1.\nA: Type 2 diabetes, above goal. Obesity.\nP: Add second agent. Program referral. Recheck in 6 months.",
                ),
            },
            Visit {
                start: "2025-07-22T10:40:00Z",
                end: "2025-07-22T11:15:00Z",
                class: "ambulatory",
                reason: "Diabetes follow-up",
                results: &[r(A1C, 7.8, "%"), r(BMI, 36.0, "kg/m2")],
                note: (
                    "Progress note",
                    "S: Attending the program weekly. Mild nausea for 2 weeks after starting the new agent, now resolved.\nO: A1c 7.8% (from 8.6%). BMI 36.0.\nA: Improving.\nP: Continue. Recheck in 6 months.",
                ),
            },
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
        visits: &[Visit {
            start: "2025-06-30T08:30:00Z",
            end: "2025-06-30T09:10:00Z",
            class: "ambulatory",
            reason: "COPD follow-up",
            results: &[r(LDL, 121.0, "mg/dL"), r(SBP, 136.0, "mm[Hg]")],
            note: (
                "Progress note",
                "S: Breathless on stairs, unchanged from last year. Quit smoking 2015.\nO: BP 136/80. LDL 121 mg/dL.\nA: COPD, stable. Hyperlipidemia on statin, near goal.\nP: Pulmonary rehab referral. Vaccines up to date. Follow up in 6 months.",
            ),
        }],
    },
];

fn field(value: &Value, key: &str) -> Result<Value, EngineError> {
    value.get(key).cloned().ok_or_else(|| EngineError::Failed(format!("demo: reply has no {key}")))
}

/// Registers the demo cohort and returns how many patients it added.
pub fn seed(session: &Session) -> Result<usize, EngineError> {
    let system = Actor::system();
    let clinician = Actor { id: "dr.demo".into(), role: healthcraft_core::Role::Clinician };
    for p in COHORT {
        let reply = session.dispatch(
            &system,
            "patient.register",
            &json!({
                "given": [p.given], "family": p.family, "birthDate": p.birth_date, "sex": p.sex,
                "address": { "city": p.city, "state": p.state, "postalCode": p.zip, "country": "US" }
            }),
        )?;
        let patient = field(&reply, "patientId")?;
        for (code, display, onset) in p.conditions {
            session.dispatch(
                &system,
                "condition.record",
                &json!({ "patientId": patient, "system": SNOMED, "code": code, "display": display, "onset": onset }),
            )?;
        }
        for v in p.visits {
            let opened =
                session.dispatch(&system, "encounter.open", &json!({ "patientId": patient, "class": v.class, "reason": v.reason, "start": v.start }))?;
            let encounter = field(&opened, "encounterId")?;
            for (code, display, value, unit) in v.results {
                session.dispatch(
                    &system,
                    "observation.record",
                    &json!({ "patientId": patient, "encounterId": encounter, "system": LOINC, "code": code, "display": display, "value": value, "unit": unit, "effective": v.start }),
                )?;
            }
            let (title, text) = v.note;
            session.dispatch(
                &clinician,
                "note.sign",
                &json!({ "patientId": patient, "encounterId": encounter, "title": title, "text": text, "signed": v.end }),
            )?;
            session.dispatch(&system, "encounter.close", &json!({ "patientId": patient, "encounterId": encounter, "end": v.end }))?;
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
        let text = serde_json::to_string(rows).unwrap();
        assert!(!text.contains("organizer"), "note text never reaches the research set");

        let dr = Actor::new("dr", Role::Clinician).unwrap();
        let list = s.dispatch(&dr, "patient.list", &Value::Null).unwrap();
        let first = &list[0]["patientId"];
        let chart = s.dispatch(&dr, "patient.chart", &json!({ "patientId": first })).unwrap();
        let encounters = chart["encounters"].as_object().unwrap();
        assert!(encounters.values().all(|e| !e["end"].is_null()), "every demo visit is closed");
        assert_eq!(chart["notes"].as_array().unwrap().len(), encounters.len());
    }
}
