//! A chart as FHIR R4 resources.

use healthcraft_record::{Chart, EncounterClass, ObservationValue, Sex};
use healthcraft_terminology as term;

use crate::resources::{
    Address, Bundle, BundleEntry, CodeableConcept, Coding, Condition, Encounter, HumanName, Meta, Observation, Patient, Period, Quantity, Reference, Resource,
};

fn coding(c: &term::Coding) -> CodeableConcept {
    CodeableConcept {
        coding: vec![Coding { system: Some(c.system.uri().to_owned()), code: Some(c.code.clone()), display: c.display.clone() }],
        text: c.display.clone(),
    }
}

fn patient_ref(chart: &Chart) -> Reference {
    Reference::to(format!("Patient/{}", chart.patient))
}

pub fn patient(chart: &Chart) -> Patient {
    let d = &chart.demographics;
    Patient {
        id: Some(chart.patient.to_string()),
        meta: Some(Meta { version_id: Some(chart.version.to_string()), last_updated: Some(chart.last_updated.to_rfc3339()) }),
        name: vec![HumanName { use_: Some("official".into()), family: Some(d.family.clone()), given: d.given.clone() }],
        gender: Some(
            match d.sex {
                Sex::Female => "female",
                Sex::Male => "male",
                Sex::Other => "other",
                Sex::Unknown => "unknown",
            }
            .into(),
        ),
        birth_date: Some(d.birth_date.to_string()),
        address: d
            .address
            .iter()
            .map(|a| Address {
                line: a.lines.clone(),
                city: a.city.clone(),
                state: a.state.clone(),
                postal_code: a.postal_code.clone(),
                country: a.country.clone(),
            })
            .collect(),
    }
}

/// HL7 v3 ActCode values FHIR R4 uses for `Encounter.class`.
pub const ACT_CODE: &str = "http://terminology.hl7.org/CodeSystem/v3-ActCode";

pub fn encounter_class_code(class: EncounterClass) -> &'static str {
    match class {
        EncounterClass::Ambulatory => "AMB",
        EncounterClass::Emergency => "EMER",
        EncounterClass::Inpatient => "IMP",
        EncounterClass::Virtual => "VR",
        EncounterClass::Home => "HH",
    }
}

pub fn encounters(chart: &Chart) -> Vec<Encounter> {
    chart
        .encounters
        .values()
        .map(|e| Encounter {
            id: Some(e.id.to_string()),
            status: Some(if e.end.is_some() { "finished" } else { "in-progress" }.into()),
            class: Some(Coding { system: Some(ACT_CODE.into()), code: Some(encounter_class_code(e.class).into()), display: None }),
            subject: Some(patient_ref(chart)),
            period: Some(Period { start: Some(e.start.to_rfc3339()), end: e.end.map(|t| t.to_rfc3339()) }),
            reason_code: e.reason.iter().map(|r| CodeableConcept { coding: vec![], text: Some(r.clone()) }).collect(),
        })
        .collect()
}

pub fn observations(chart: &Chart) -> Vec<Observation> {
    chart
        .observations
        .iter()
        .map(|o| {
            let mut obs = Observation {
                id: Some(o.id.to_string()),
                status: Some(if o.entered_in_error.is_some() { "entered-in-error" } else { "final" }.into()),
                code: coding(&o.code),
                subject: Some(patient_ref(chart)),
                encounter: o.encounter.map(|e| Reference::to(format!("Encounter/{e}"))),
                effective_date_time: Some(o.effective.to_rfc3339()),
                ..Observation::default()
            };
            match &o.value {
                ObservationValue::Quantity { value, unit } => obs.value_quantity = Some(Quantity { value: Some(*value), unit: Some(unit.clone()) }),
                ObservationValue::Text(text) => obs.value_string = Some(text.clone()),
                ObservationValue::Coded(c) => obs.value_codeable_concept = Some(coding(c)),
            }
            obs
        })
        .collect()
}

pub fn conditions(chart: &Chart) -> Vec<Condition> {
    chart
        .conditions
        .iter()
        .map(|c| Condition {
            id: Some(c.id.to_string()),
            code: coding(&c.code),
            subject: Some(patient_ref(chart)),
            onset_date_time: c.onset.map(|d| d.to_string()),
        })
        .collect()
}

/// Everything in the chart as one `collection` bundle (what `Patient/$everything` returns).
pub fn everything(chart: &Chart) -> Bundle {
    let mut entry = vec![BundleEntry { full_url: None, resource: Resource::Patient(patient(chart)) }];
    entry.extend(encounters(chart).into_iter().map(|r| BundleEntry { full_url: None, resource: Resource::Encounter(r) }));
    entry.extend(observations(chart).into_iter().map(|r| BundleEntry { full_url: None, resource: Resource::Observation(r) }));
    entry.extend(conditions(chart).into_iter().map(|r| BundleEntry { full_url: None, resource: Resource::Condition(r) }));
    Bundle { type_: Some("collection".into()), total: None, entry }
}

/// Patients as a `searchset` bundle (what `GET /Patient` returns).
pub fn searchset(patients: Vec<Patient>) -> Bundle {
    let total = u64::try_from(patients.len()).ok();
    Bundle {
        type_: Some("searchset".into()),
        total,
        entry: patients.into_iter().map(|p| BundleEntry { full_url: None, resource: Resource::Patient(p) }).collect(),
    }
}
