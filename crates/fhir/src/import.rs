//! FHIR bundles as clinical events.
//!
//! Built for the bundles [Synthea](https://github.com/synthetichealth/synthea) writes (one
//! transaction bundle per synthetic patient, entries referenced by `urn:uuid:`), and works for any
//! bundle that carries Patients with their Encounters, Observations and Conditions. Resources it
//! cannot use are listed in [`ImportReport::skipped`] with the reason, never dropped silently.

use std::collections::BTreeMap;

use healthcraft_core::{ConditionId, EncounterId, IdSource, ObservationId, PatientId, Timestamp, Uuid};
use healthcraft_record::{Address, ClinicalEvent, Demographics, EncounterClass, ObservationValue, Sex};
use healthcraft_terminology::{CodeSystem, Coding};
use serde::{Deserialize, Serialize};

use crate::resources::{self, CodeableConcept, Reference, Resource};
use crate::{parse_date, parse_datetime};

/// Largest bundle accepted, in bytes (Synthea's are a few MB each).
pub const MAX_BUNDLE_BYTES: usize = 128 << 20;
/// Most entries in one bundle.
pub const MAX_ENTRIES: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    #[error("bundle is larger than {MAX_BUNDLE_BYTES} bytes")]
    TooLarge,
    #[error("bundle has more than {MAX_ENTRIES} entries")]
    TooManyEntries,
    #[error("not valid JSON: {0}")]
    Json(String),
    #[error("expected a FHIR Bundle, found resourceType `{0}`")]
    NotABundle(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedPatient {
    pub patient: PatientId,
    /// In an order the record accepts: registration, conditions, then each encounter with its
    /// observations, then observations outside any encounter.
    pub events: Vec<ClinicalEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub resource_type: String,
    pub id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub patients: Vec<ImportedPatient>,
    pub skipped: Vec<Skipped>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBundle {
    resource_type: String,
    #[serde(default)]
    entry: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEntry {
    full_url: Option<String>,
    #[serde(default)]
    resource: serde_json::Value,
}

struct PendingEncounter {
    id: EncounterId,
    patient: PatientId,
    class: EncounterClass,
    start: Timestamp,
    end: Option<Timestamp>,
    reason: Option<String>,
    observations: Vec<(Timestamp, ClinicalEvent)>,
}

#[derive(Default)]
struct PendingPatient {
    register: Option<ClinicalEvent>,
    conditions: Vec<ClinicalEvent>,
    encounters: Vec<EncounterId>,
    loose_observations: Vec<(Timestamp, ClinicalEvent)>,
}

/// Turns a bundle into per-patient event lists. Nothing is stored: the caller appends the events
/// (through the engine, so the import is authorized and audited).
pub fn import_bundle(json: &str, ids: &dyn IdSource) -> Result<ImportReport, ImportError> {
    if json.len() > MAX_BUNDLE_BYTES {
        return Err(ImportError::TooLarge);
    }
    let raw: RawBundle = serde_json::from_str(json).map_err(|e| ImportError::Json(e.to_string()))?;
    if raw.resource_type != "Bundle" {
        return Err(ImportError::NotABundle(raw.resource_type.chars().take(64).collect()));
    }
    if raw.entry.len() > MAX_ENTRIES {
        return Err(ImportError::TooManyEntries);
    }

    let mut report = ImportReport::default();
    let mut parsed = Vec::with_capacity(raw.entry.len());
    for entry in raw.entry {
        let resource_type = entry.resource.get("resourceType").and_then(|v| v.as_str()).unwrap_or("(none)").chars().take(64).collect::<String>();
        let id = entry.resource.get("id").and_then(|v| v.as_str()).map(|s| s.chars().take(128).collect::<String>());
        match serde_json::from_value::<Resource>(entry.resource) {
            Ok(Resource::Other) => report.skipped.push(Skipped { resource_type, id, reason: "resource type not imported yet".into() }),
            Ok(resource) => parsed.push((entry.full_url, resource)),
            Err(e) => report.skipped.push(Skipped { resource_type, id, reason: format!("unreadable: {e}") }),
        }
    }

    // Pass 1: patients. Every name a later resource may use to point at one (`urn:uuid:…`,
    // `Patient/…`) maps to its new id.
    let mut patient_refs: BTreeMap<String, PatientId> = BTreeMap::new();
    let mut pending: BTreeMap<PatientId, PendingPatient> = BTreeMap::new();
    let mut order: Vec<PatientId> = Vec::new();
    for (full_url, resource) in &parsed {
        let Resource::Patient(p) = resource else { continue };
        let patient = PatientId(stable_uuid(p.id.as_deref(), ids));
        match demographics(p) {
            Ok(demographics) => {
                for name in ref_names("Patient", full_url.as_deref(), p.id.as_deref()) {
                    patient_refs.insert(name, patient);
                }
                let slot = pending.entry(patient).or_default();
                slot.register = Some(ClinicalEvent::PatientRegistered { patient, demographics });
                order.push(patient);
            }
            Err(reason) => report.skipped.push(Skipped { resource_type: "Patient".into(), id: p.id.clone(), reason }),
        }
    }

    // Pass 2: encounters.
    let mut encounter_refs: BTreeMap<String, EncounterId> = BTreeMap::new();
    let mut encounters: BTreeMap<EncounterId, PendingEncounter> = BTreeMap::new();
    for (full_url, resource) in &parsed {
        let Resource::Encounter(e) = resource else { continue };
        let skip = |reason: &str| Skipped { resource_type: "Encounter".into(), id: e.id.clone(), reason: reason.into() };
        let Some(patient) = resolve(&patient_refs, e.subject.as_ref()) else {
            report.skipped.push(skip("subject is not a patient in this bundle"));
            continue;
        };
        let Some(start) = e.period.as_ref().and_then(|p| p.start.as_deref()).and_then(parse_datetime) else {
            report.skipped.push(skip("no readable period.start"));
            continue;
        };
        let end = e.period.as_ref().and_then(|p| p.end.as_deref()).and_then(parse_datetime).filter(|end| *end >= start);
        let id = EncounterId(stable_uuid(e.id.as_deref(), ids));
        if encounters.contains_key(&id) {
            report.skipped.push(skip("duplicate encounter id"));
            continue;
        }
        for name in ref_names("Encounter", full_url.as_deref(), e.id.as_deref()) {
            encounter_refs.insert(name, id);
        }
        let class = encounter_class(e.class.as_ref().and_then(|c| c.code.as_deref()));
        let reason = e.reason_code.first().and_then(concept_text);
        encounters.insert(id, PendingEncounter { id, patient, class, start, end, reason, observations: Vec::new() });
        pending.entry(patient).or_default().encounters.push(id);
    }

    // Pass 3: observations and conditions.
    for (_, resource) in &parsed {
        match resource {
            Resource::Observation(o) => {
                let skip = |reason: String| Skipped { resource_type: "Observation".into(), id: o.id.clone(), reason };
                let Some(patient) = resolve(&patient_refs, o.subject.as_ref()) else {
                    report.skipped.push(skip("subject is not a patient in this bundle".into()));
                    continue;
                };
                let code = match first_coding(&o.code) {
                    Ok(code) => code,
                    Err(reason) => {
                        report.skipped.push(skip(reason));
                        continue;
                    }
                };
                let value = if let Some(q) = &o.value_quantity {
                    match q.value {
                        Some(v) if v.is_finite() => ObservationValue::Quantity { value: v, unit: q.unit.clone().unwrap_or_default() },
                        _ => {
                            report.skipped.push(skip("valueQuantity has no number".into()));
                            continue;
                        }
                    }
                } else if let Some(s) = &o.value_string {
                    ObservationValue::Text(s.clone())
                } else if let Some(cc) = &o.value_codeable_concept {
                    match first_coding(cc) {
                        Ok(c) => ObservationValue::Coded(c),
                        Err(reason) => {
                            report.skipped.push(skip(reason));
                            continue;
                        }
                    }
                } else {
                    // Blood pressure and other panels carry their values in `component`.
                    report.skipped.push(skip("no value[x] (component observations are not imported yet)".into()));
                    continue;
                };
                let encounter = resolve(&encounter_refs, o.encounter.as_ref()).filter(|e| encounters.get(e).is_some_and(|pe| pe.patient == patient));
                let Some(effective) =
                    o.effective_date_time.as_deref().and_then(parse_datetime).or_else(|| encounter.and_then(|e| encounters.get(&e)).map(|e| e.start))
                else {
                    report.skipped.push(skip("no readable effectiveDateTime".into()));
                    continue;
                };
                let event = ClinicalEvent::ObservationRecorded {
                    patient,
                    observation: ObservationId(stable_uuid(o.id.as_deref(), ids)),
                    encounter,
                    code,
                    value,
                    effective,
                };
                if let Err(e) = event.validate() {
                    report.skipped.push(skip(e.to_string()));
                    continue;
                }
                match encounter.and_then(|e| encounters.get_mut(&e)) {
                    Some(pe) => pe.observations.push((effective, event)),
                    None => pending.entry(patient).or_default().loose_observations.push((effective, event)),
                }
            }
            Resource::Condition(c) => {
                let skip = |reason: String| Skipped { resource_type: "Condition".into(), id: c.id.clone(), reason };
                let Some(patient) = resolve(&patient_refs, c.subject.as_ref()) else {
                    report.skipped.push(skip("subject is not a patient in this bundle".into()));
                    continue;
                };
                match first_coding(&c.code) {
                    Ok(code) => pending.entry(patient).or_default().conditions.push(ClinicalEvent::ConditionRecorded {
                        patient,
                        condition: ConditionId(stable_uuid(c.id.as_deref(), ids)),
                        code,
                        onset: c.onset_date_time.as_deref().and_then(parse_date),
                    }),
                    Err(reason) => report.skipped.push(skip(reason)),
                }
            }
            _ => {}
        }
    }

    for patient in order {
        let Some(mut p) = pending.remove(&patient) else { continue };
        let Some(register) = p.register.take() else { continue };
        let mut events = vec![register];
        events.append(&mut p.conditions);
        let mut visits: Vec<PendingEncounter> = p.encounters.iter().filter_map(|id| encounters.remove(id)).collect();
        visits.sort_by_key(|e| e.start);
        for mut visit in visits {
            events.push(ClinicalEvent::EncounterOpened { patient, encounter: visit.id, class: visit.class, start: visit.start, reason: visit.reason });
            visit.observations.sort_by_key(|(t, _)| *t);
            events.extend(visit.observations.into_iter().map(|(_, e)| e));
            if let Some(end) = visit.end {
                events.push(ClinicalEvent::EncounterClosed { patient, encounter: visit.id, end });
            }
        }
        p.loose_observations.sort_by_key(|(t, _)| *t);
        events.extend(p.loose_observations.into_iter().map(|(_, e)| e));
        report.patients.push(ImportedPatient { patient, events });
    }
    Ok(report)
}

/// Keeps the source's id when it is a UUID (Synthea's are), so re-importing a bundle names the same
/// records; otherwise mints a new one.
fn stable_uuid(id: Option<&str>, ids: &dyn IdSource) -> Uuid {
    id.and_then(|s| Uuid::parse_str(s).ok()).unwrap_or_else(|| ids.next_uuid())
}

fn ref_names(resource_type: &str, full_url: Option<&str>, id: Option<&str>) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(url) = full_url {
        names.push(url.to_owned());
    }
    if let Some(id) = id {
        names.push(format!("{resource_type}/{id}"));
        names.push(format!("urn:uuid:{id}"));
    }
    names
}

fn resolve<T: Copy>(refs: &BTreeMap<String, T>, reference: Option<&Reference>) -> Option<T> {
    refs.get(reference?.reference.as_deref()?).copied()
}

fn concept_text(cc: &CodeableConcept) -> Option<String> {
    cc.text.clone().or_else(|| cc.coding.iter().find_map(|c| c.display.clone()))
}

fn first_coding(cc: &CodeableConcept) -> Result<Coding, String> {
    let c = cc.coding.iter().find(|c| c.system.is_some() && c.code.is_some()).ok_or_else(|| "code has no coding with system and code".to_owned())?;
    let system = CodeSystem::from_uri(c.system.as_deref().unwrap_or_default()).map_err(|e| e.to_string())?;
    Coding::new(system, c.code.clone().unwrap_or_default(), c.display.clone().or_else(|| cc.text.clone())).map_err(|e| e.to_string())
}

fn encounter_class(code: Option<&str>) -> EncounterClass {
    match code {
        Some("EMER") => EncounterClass::Emergency,
        Some("IMP" | "ACUTE" | "NONAC" | "OBSENC") => EncounterClass::Inpatient,
        Some("VR") => EncounterClass::Virtual,
        Some("HH") => EncounterClass::Home,
        _ => EncounterClass::Ambulatory,
    }
}

fn demographics(p: &resources::Patient) -> Result<Demographics, String> {
    let name = p.name.iter().find(|n| n.use_.as_deref() == Some("official")).or_else(|| p.name.first()).ok_or("no name")?;
    let family = name.family.clone().filter(|f| !f.trim().is_empty()).ok_or("no family name")?;
    let birth_date = p.birth_date.as_deref().and_then(parse_date).ok_or("no readable birthDate")?;
    let sex = match p.gender.as_deref() {
        Some("female") => Sex::Female,
        Some("male") => Sex::Male,
        Some("other") => Sex::Other,
        _ => Sex::Unknown,
    };
    let address = p.address.first().map(|a| Address {
        lines: a.line.clone(),
        city: a.city.clone(),
        state: a.state.clone(),
        postal_code: a.postal_code.clone(),
        country: a.country.clone(),
    });
    let d = Demographics { given: name.given.clone(), family, birth_date, sex, address };
    d.validate().map_err(|e| e.to_string())?;
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export;
    use healthcraft_core::{Actor, EventId, SequentialIds};
    use healthcraft_record::{Chart, RecordedEvent};

    const BUNDLE: &str = include_str!("../../../fixtures/synthetic-bundle.json");

    fn replay(events: &[ClinicalEvent]) -> Chart {
        let recorded: Vec<RecordedEvent> = events
            .iter()
            .enumerate()
            .map(|(i, e)| RecordedEvent {
                id: EventId(Uuid::from_u128(i as u128)),
                sequence: i as u64 + 1,
                recorded_at: Timestamp::from_timestamp(0, 0).unwrap(),
                actor: Actor::system(),
                event: e.clone(),
            })
            .collect();
        Chart::replay(&recorded).unwrap()
    }

    #[test]
    fn imports_the_synthetic_fixture() {
        let report = import_bundle(BUNDLE, &SequentialIds::default()).unwrap();
        assert_eq!(report.patients.len(), 1);
        let chart = replay(&report.patients[0].events);
        assert_eq!(chart.patient.to_string(), "8f1d7c2e-0000-4000-8000-000000000001");
        assert_eq!(chart.demographics.display_name(), "Synthetic Testpatient");
        assert_eq!(chart.encounters.len(), 2);
        assert_eq!(chart.observations.len(), 3);
        assert_eq!(chart.conditions.len(), 1);
        assert_eq!(chart.conditions[0].code.code, "44054006");
        // The blood-pressure panel and the unknown resource type are reported, not dropped.
        let reasons: Vec<&str> = report.skipped.iter().map(|s| s.resource_type.as_str()).collect();
        assert_eq!(reasons, vec!["Organization", "Observation"]);
    }

    #[test]
    fn import_then_export_round_trips_the_essentials() {
        let report = import_bundle(BUNDLE, &SequentialIds::default()).unwrap();
        let chart = replay(&report.patients[0].events);
        let patient = export::patient(&chart);
        assert_eq!(patient.birth_date.as_deref(), Some("1961-04-12"));
        assert_eq!(patient.gender.as_deref(), Some("female"));
        let observations = export::observations(&chart);
        assert!(observations.iter().any(|o| o.code.coding[0].code.as_deref() == Some("4548-4") && o.value_quantity.as_ref().unwrap().value == Some(8.1)));
        let bundle = export::everything(&chart);
        let json = serde_json::to_value(Resource::Bundle(bundle)).unwrap();
        assert_eq!(json["resourceType"], "Bundle");
        assert_eq!(json["entry"][0]["resource"]["resourceType"], "Patient");
    }

    #[test]
    fn rejects_non_bundles_and_garbage() {
        let ids = SequentialIds::default();
        assert!(matches!(import_bundle("{", &ids), Err(ImportError::Json(_))));
        assert!(matches!(import_bundle("[]", &ids), Err(ImportError::Json(_))));
        assert_eq!(import_bundle(r#"{"resourceType":"Patient"}"#, &ids), Err(ImportError::NotABundle("Patient".into())));
        let empty = import_bundle(r#"{"resourceType":"Bundle"}"#, &ids).unwrap();
        assert!(empty.patients.is_empty());
    }

    #[test]
    fn skips_unusable_resources_with_reasons() {
        let json = r#"{"resourceType":"Bundle","entry":[
            {"resource":{"resourceType":"Patient","id":"p1","name":[{"given":["No"]}],"birthDate":"2000-01-01"}},
            {"resource":{"resourceType":"Observation","id":"o1","subject":{"reference":"Patient/nobody"},"code":{"coding":[{"system":"http://loinc.org","code":"1-8"}]},"valueString":"x"}},
            {"resource":{"resourceType":"Encounter","id":"e1","subject":{"reference":"Patient/p1"}}},
            {"resource":{"resourceType":"Observation","id":7}}
        ]}"#;
        let report = import_bundle(json, &SequentialIds::default()).unwrap();
        assert!(report.patients.is_empty());
        let reasons: Vec<&str> = report.skipped.iter().map(|s| s.reason.as_str()).collect();
        assert_eq!(reasons.len(), 4, "{reasons:?}");
        assert!(reasons.iter().any(|r| r.starts_with("unreadable")));
        assert!(reasons.contains(&"no family name"));
    }
}
