//! HIPAA Safe Harbor de-identification (45 CFR 164.514(b)(2)).
//!
//! Safe Harbor removes 18 kinds of identifiers. A [`DeidentifiedPatient`] is built field by field
//! from what the rule allows, rather than by deleting what it forbids, so a field added to the
//! record later cannot leak through by default:
//!
//! - names, street addresses, cities, contact details, record numbers, device ids: never copied;
//! - **dates**: only the year is kept (birth, visits, observations, onsets);
//! - **ages over 89**: the birth year is withheld and the patient is flagged `90+`;
//! - **ZIP codes**: the first three digits, or `000` where that 3-digit area has 20,000 people or
//!   fewer ([`RESTRICTED_ZIP3`]);
//! - **free text** (text observation values, visit reasons): dropped, because it can contain any
//!   identifier;
//! - **the research id** is random, not derived from the patient's id or any other identifier
//!   (164.514(c)); the key that links it back stays with the covered entity in [`Pseudonymizer`].
//!
//! Safe Harbor also requires that the organization has no actual knowledge that the remaining data
//! could identify someone. That is an organizational duty this code cannot discharge.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use healthcraft_core::{IdSource, PatientId, Uuid};
use healthcraft_record::{Chart, EncounterClass, ObservationValue, Sex};
use healthcraft_terminology::Coding;
use serde::Serialize;

/// 3-digit ZIP prefixes whose areas had 20,000 people or fewer in the 2000 Census, as listed in
/// HHS's de-identification guidance. These become `000`. Re-check against current census data
/// before production use.
pub const RESTRICTED_ZIP3: &[&str] = &["036", "059", "063", "102", "203", "556", "692", "790", "821", "823", "830", "831", "878", "879", "884", "890", "893"];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeidentifiedPatient {
    pub research_id: Uuid,
    pub sex: Sex,
    /// `None` when the patient is 90 or older.
    pub birth_year: Option<i32>,
    pub age_90_or_over: bool,
    pub state: Option<String>,
    pub zip3: Option<String>,
    pub encounters: Vec<DeidentifiedEncounter>,
    pub observations: Vec<DeidentifiedObservation>,
    pub conditions: Vec<DeidentifiedCondition>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeidentifiedEncounter {
    pub class: EncounterClass,
    pub year: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeidentifiedObservation {
    pub code: Coding,
    /// `None` for free-text values, which are removed.
    pub value: Option<ObservationValue>,
    pub year: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeidentifiedCondition {
    pub code: Coding,
    pub onset_year: Option<i32>,
}

/// Assigns each patient a random research id and keeps the link. The link is the re-identification
/// key: it never leaves the covered entity and is never part of a de-identified data set.
#[derive(Debug, Default)]
pub struct Pseudonymizer {
    key: BTreeMap<PatientId, Uuid>,
}

impl Pseudonymizer {
    pub fn research_id(&mut self, patient: PatientId, ids: &dyn IdSource) -> Uuid {
        *self.key.entry(patient).or_insert_with(|| ids.next_uuid())
    }
}

/// Whole years between `birth` and `as_of`.
pub fn age_on(birth: NaiveDate, as_of: NaiveDate) -> i32 {
    let mut age = as_of.year().saturating_sub(birth.year());
    if (as_of.month(), as_of.day()) < (birth.month(), birth.day()) {
        age = age.saturating_sub(1);
    }
    age
}

pub fn zip3(postal_code: &str) -> Option<String> {
    let digits: String = postal_code.chars().take_while(char::is_ascii_digit).take(3).collect();
    if digits.len() != 3 {
        return None;
    }
    Some(if RESTRICTED_ZIP3.contains(&digits.as_str()) { "000".to_owned() } else { digits })
}

/// De-identifies one chart, as of `as_of` (the date ages are computed on).
pub fn deidentify(chart: &Chart, as_of: NaiveDate, pseudonyms: &mut Pseudonymizer, ids: &dyn IdSource) -> DeidentifiedPatient {
    let d = &chart.demographics;
    let age_90_or_over = age_on(d.birth_date, as_of) > 89;
    DeidentifiedPatient {
        research_id: pseudonyms.research_id(chart.patient, ids),
        sex: d.sex,
        birth_year: (!age_90_or_over).then(|| d.birth_date.year()),
        age_90_or_over,
        state: d.address.as_ref().and_then(|a| a.state.clone()),
        zip3: d.address.as_ref().and_then(|a| a.postal_code.as_deref()).and_then(zip3),
        encounters: chart.encounters.values().map(|e| DeidentifiedEncounter { class: e.class, year: e.start.year() }).collect(),
        observations: chart
            .active_observations()
            .map(|o| DeidentifiedObservation {
                code: o.code.clone(),
                value: match &o.value {
                    ObservationValue::Text(_) => None,
                    other => Some(other.clone()),
                },
                year: o.effective.year(),
            })
            .collect(),
        conditions: chart.conditions.iter().map(|c| DeidentifiedCondition { code: c.code.clone(), onset_year: c.onset.map(|d| d.year()) }).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use healthcraft_core::{Actor, ConditionId, EventId, IdSource, ObservationId, SequentialIds, Timestamp};
    use healthcraft_record::{Address, ClinicalEvent, Demographics, RecordedEvent};
    use healthcraft_terminology::CodeSystem;

    fn chart(birth: NaiveDate, postal: &str) -> Chart {
        let ids = SequentialIds::default();
        let p = PatientId::new(&ids);
        let at = Timestamp::from_timestamp(1_750_000_000, 0).unwrap();
        let events = [
            ClinicalEvent::PatientRegistered {
                patient: p,
                demographics: Demographics {
                    given: vec!["Synthetic".into()],
                    family: "Testpatient".into(),
                    birth_date: birth,
                    sex: Sex::Female,
                    address: Some(Address {
                        lines: vec!["1 Example Way".into()],
                        city: Some("Springfield".into()),
                        state: Some("MA".into()),
                        postal_code: Some(postal.into()),
                        country: Some("US".into()),
                    }),
                },
            },
            ClinicalEvent::ObservationRecorded {
                patient: p,
                observation: ObservationId::new(&ids),
                encounter: None,
                code: Coding::new(CodeSystem::Loinc, "8251-1", None).unwrap(),
                value: ObservationValue::Text("Patient Jane Doe called from 555-0100".into()),
                effective: at,
            },
            ClinicalEvent::ConditionRecorded {
                patient: p,
                condition: ConditionId::new(&ids),
                code: Coding::new(CodeSystem::SnomedCt, "44054006", None).unwrap(),
                onset: NaiveDate::from_ymd_opt(2019, 6, 20),
            },
        ];
        let recorded: Vec<RecordedEvent> = events
            .into_iter()
            .enumerate()
            .map(|(i, event)| RecordedEvent { id: EventId(Uuid::from_u128(i as u128)), sequence: i as u64 + 1, recorded_at: at, actor: Actor::system(), event })
            .collect();
        Chart::replay(&recorded).unwrap()
    }

    #[test]
    fn keeps_only_what_safe_harbor_allows() {
        let c = chart(NaiveDate::from_ymd_opt(1961, 4, 12).unwrap(), "01103-1234");
        let mut key = Pseudonymizer::default();
        // A separate id source from the chart's, as in production.
        let ids = SequentialIds::default();
        for _ in 0..1000 {
            ids.next_uuid();
        }
        let out = deidentify(&c, NaiveDate::from_ymd_opt(2026, 10, 27).unwrap(), &mut key, &ids);
        assert_eq!(out.birth_year, Some(1961));
        assert!(!out.age_90_or_over);
        assert_eq!(out.zip3.as_deref(), Some("011"));
        assert_eq!(out.state.as_deref(), Some("MA"));
        assert_eq!(out.observations[0].value, None, "free text is dropped");
        assert_eq!(out.conditions[0].onset_year, Some(2019));
        assert_ne!(out.research_id, c.patient.0, "research id is not the patient id");
        let json = serde_json::to_string(&out).unwrap();
        for identifier in ["Testpatient", "Synthetic", "Example Way", "Springfield", "01103", "555-0100", "1961-04-12", &c.patient.to_string()] {
            assert!(!json.contains(identifier), "{identifier} leaked: {json}");
        }
    }

    #[test]
    fn aggregates_ages_over_89() {
        let c = chart(NaiveDate::from_ymd_opt(1936, 10, 28).unwrap(), "01103");
        let ids = SequentialIds::default();
        let as_of = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        assert_eq!(age_on(c.demographics.birth_date, as_of), 89);
        let out = deidentify(&c, as_of, &mut Pseudonymizer::default(), &ids);
        assert_eq!(out.birth_year, Some(1936), "89 is still reported");
        let out = deidentify(&c, NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(), &mut Pseudonymizer::default(), &ids);
        assert_eq!(out.birth_year, None, "90 is aggregated");
        assert!(out.age_90_or_over);
    }

    #[test]
    fn zip3_rules() {
        assert_eq!(zip3("02139").as_deref(), Some("021"));
        assert_eq!(zip3("03601").as_deref(), Some("000"));
        assert_eq!(zip3("89301").as_deref(), Some("000"));
        assert_eq!(zip3("12"), None);
        assert_eq!(zip3("SW1A 1AA"), None);
    }

    #[test]
    fn research_ids_are_stable_per_patient() {
        let ids = SequentialIds::default();
        let mut key = Pseudonymizer::default();
        let a = PatientId(Uuid::from_u128(100));
        let b = PatientId(Uuid::from_u128(200));
        let first = key.research_id(a, &ids);
        assert_eq!(key.research_id(a, &ids), first);
        assert_ne!(key.research_id(b, &ids), first);
    }
}
