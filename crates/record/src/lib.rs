//! The clinical record.
//!
//! A patient's record is an append-only stream of [`ClinicalEvent`]s. Nothing is ever updated in
//! place or deleted: a correction is a new event (`DemographicsAmended`,
//! `ObservationEnteredInError`) that says what changed, who changed it and why. This is how medical
//! records are kept on paper, it gives a complete history for free, and it lets every other view
//! (the chart, the FHIR API, the research warehouse) be a projection that can be rebuilt at any
//! time.
//!
//! [`Chart`] is the projection clinicians read. [`Chart::apply`] also enforces the record's
//! invariants, so the store runs every new event through it before accepting it.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use healthcraft_core::limits::{self, MAX_ITEMS, MAX_LONG_TEXT, MAX_SHORT_TEXT};
use healthcraft_core::{Actor, ConditionId, CoreError, EncounterId, EventId, ObservationId, PatientId, Timestamp};
use healthcraft_terminology::{Coding, TerminologyError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecordError {
    #[error(transparent)]
    Invalid(#[from] CoreError),
    #[error(transparent)]
    Code(#[from] TerminologyError),
    #[error("the first event of a record must register the patient")]
    NotRegistered,
    #[error("patient is already registered")]
    AlreadyRegistered,
    #[error("event belongs to patient {event}, not {stream}")]
    WrongPatient { stream: PatientId, event: PatientId },
    #[error("encounter {0} is not open on this record")]
    EncounterNotOpen(EncounterId),
    #[error("encounter {0} already exists")]
    DuplicateEncounter(EncounterId),
    #[error("observation {0} already exists")]
    DuplicateObservation(ObservationId),
    #[error("observation {0} is not on this record")]
    UnknownObservation(ObservationId),
    #[error("condition {0} already exists")]
    DuplicateCondition(ConditionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sex {
    Female,
    Male,
    Other,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Address {
    #[serde(default)]
    pub lines: Vec<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Demographics {
    pub given: Vec<String>,
    pub family: String,
    pub birth_date: NaiveDate,
    pub sex: Sex,
    #[serde(default)]
    pub address: Option<Address>,
}

impl Demographics {
    pub fn validate(&self) -> Result<(), RecordError> {
        limits::check_required("family", &self.family, MAX_SHORT_TEXT)?;
        if self.given.len() > MAX_ITEMS {
            return Err(CoreError::Invalid { field: "given", reason: format!("more than {MAX_ITEMS} names") }.into());
        }
        for given in &self.given {
            limits::check_required("given", given, MAX_SHORT_TEXT)?;
        }
        if let Some(address) = &self.address {
            if address.lines.len() > MAX_ITEMS {
                return Err(CoreError::Invalid { field: "address.lines", reason: format!("more than {MAX_ITEMS} lines") }.into());
            }
            for line in &address.lines {
                limits::check_text("address.lines", line, MAX_SHORT_TEXT)?;
            }
            for (field, value) in [
                ("address.city", &address.city),
                ("address.state", &address.state),
                ("address.postalCode", &address.postal_code),
                ("address.country", &address.country),
            ] {
                if let Some(value) = value {
                    limits::check_text(field, value, MAX_SHORT_TEXT)?;
                }
            }
        }
        Ok(())
    }

    /// "Given Family".
    pub fn display_name(&self) -> String {
        let mut parts: Vec<&str> = self.given.iter().map(String::as_str).collect();
        parts.push(&self.family);
        parts.join(" ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EncounterClass {
    Ambulatory,
    Emergency,
    Inpatient,
    Virtual,
    Home,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "value")]
pub enum ObservationValue {
    Quantity { value: f64, unit: String },
    Text(String),
    Coded(Coding),
}

impl ObservationValue {
    pub fn validate(&self) -> Result<(), RecordError> {
        match self {
            Self::Quantity { value, unit } => {
                if !value.is_finite() {
                    return Err(CoreError::Invalid { field: "value", reason: "must be a finite number".into() }.into());
                }
                limits::check_text("unit", unit, MAX_SHORT_TEXT)?;
            }
            Self::Text(text) => limits::check_text("value", text, MAX_LONG_TEXT)?,
            Self::Coded(coding) => coding.validate()?,
        }
        Ok(())
    }

    pub fn display(&self) -> String {
        match self {
            Self::Quantity { value, unit } if unit.is_empty() => format!("{value}"),
            Self::Quantity { value, unit } => format!("{value} {unit}"),
            Self::Text(text) => text.clone(),
            Self::Coded(coding) => coding.label(),
        }
    }
}

/// Something that happened to a patient's record. Events are facts: they are validated once,
/// stored forever, and never edited.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ClinicalEvent {
    #[serde(rename_all = "camelCase")]
    PatientRegistered { patient: PatientId, demographics: Demographics },
    #[serde(rename_all = "camelCase")]
    DemographicsAmended { patient: PatientId, demographics: Demographics, reason: String },
    #[serde(rename_all = "camelCase")]
    EncounterOpened { patient: PatientId, encounter: EncounterId, class: EncounterClass, start: Timestamp, reason: Option<String> },
    #[serde(rename_all = "camelCase")]
    EncounterClosed { patient: PatientId, encounter: EncounterId, end: Timestamp },
    #[serde(rename_all = "camelCase")]
    ObservationRecorded {
        patient: PatientId,
        observation: ObservationId,
        encounter: Option<EncounterId>,
        code: Coding,
        value: ObservationValue,
        effective: Timestamp,
    },
    #[serde(rename_all = "camelCase")]
    ObservationEnteredInError { patient: PatientId, observation: ObservationId, reason: String },
    #[serde(rename_all = "camelCase")]
    ConditionRecorded { patient: PatientId, condition: ConditionId, code: Coding, onset: Option<NaiveDate> },
}

impl ClinicalEvent {
    pub fn patient(&self) -> PatientId {
        match self {
            Self::PatientRegistered { patient, .. }
            | Self::DemographicsAmended { patient, .. }
            | Self::EncounterOpened { patient, .. }
            | Self::EncounterClosed { patient, .. }
            | Self::ObservationRecorded { patient, .. }
            | Self::ObservationEnteredInError { patient, .. }
            | Self::ConditionRecorded { patient, .. } => *patient,
        }
    }

    /// Checks the event on its own (sizes, codes, numbers). [`Chart::apply`] checks it against the
    /// record.
    pub fn validate(&self) -> Result<(), RecordError> {
        match self {
            Self::PatientRegistered { demographics, .. } => demographics.validate(),
            Self::DemographicsAmended { demographics, reason, .. } => {
                demographics.validate()?;
                Ok(limits::check_required("reason", reason, MAX_LONG_TEXT)?)
            }
            Self::EncounterOpened { reason, .. } => match reason {
                Some(reason) => Ok(limits::check_text("reason", reason, MAX_LONG_TEXT)?),
                None => Ok(()),
            },
            Self::EncounterClosed { .. } => Ok(()),
            Self::ObservationRecorded { code, value, .. } => {
                code.validate()?;
                value.validate()
            }
            Self::ObservationEnteredInError { reason, .. } => Ok(limits::check_required("reason", reason, MAX_LONG_TEXT)?),
            Self::ConditionRecorded { code, .. } => Ok(code.validate()?),
        }
    }
}

/// An event as stored: its position in the patient's stream, when it was recorded and by whom.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedEvent {
    pub id: EventId,
    /// 1-based position in the patient's stream.
    pub sequence: u64,
    pub recorded_at: Timestamp,
    pub actor: Actor,
    pub event: ClinicalEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Encounter {
    pub id: EncounterId,
    pub class: EncounterClass,
    pub start: Timestamp,
    pub end: Option<Timestamp>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    pub id: ObservationId,
    pub encounter: Option<EncounterId>,
    pub code: Coding,
    pub value: ObservationValue,
    pub effective: Timestamp,
    /// Set when the observation was marked entered-in-error; it stays in the chart, struck out.
    pub entered_in_error: Option<String>,
    pub recorded_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub id: ConditionId,
    pub code: Coding,
    pub onset: Option<NaiveDate>,
}

/// The current state of one patient's record, rebuilt from its events.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub patient: PatientId,
    pub demographics: Demographics,
    pub encounters: BTreeMap<EncounterId, Encounter>,
    /// In recording order.
    pub observations: Vec<Observation>,
    pub conditions: Vec<Condition>,
    /// How many events the chart was built from (the stream's version).
    pub version: u64,
    pub last_updated: Timestamp,
}

impl Chart {
    /// Rebuilds a chart from a stream. The stream must start with `PatientRegistered`.
    pub fn replay<'a>(events: impl IntoIterator<Item = &'a RecordedEvent>) -> Result<Self, RecordError> {
        let mut chart: Option<Chart> = None;
        for recorded in events {
            match chart.as_mut() {
                Some(c) => c.apply(recorded)?,
                None => chart = Some(Self::start(recorded)?),
            }
        }
        chart.ok_or(RecordError::NotRegistered)
    }

    /// Starts a chart from the stream's first event.
    pub fn start(recorded: &RecordedEvent) -> Result<Self, RecordError> {
        recorded.event.validate()?;
        let ClinicalEvent::PatientRegistered { patient, demographics } = &recorded.event else {
            return Err(RecordError::NotRegistered);
        };
        Ok(Self {
            patient: *patient,
            demographics: demographics.clone(),
            encounters: BTreeMap::new(),
            observations: Vec::new(),
            conditions: Vec::new(),
            version: 1,
            last_updated: recorded.recorded_at,
        })
    }

    /// Applies the next event, or explains why the record cannot accept it. The chart is unchanged
    /// on error.
    pub fn apply(&mut self, recorded: &RecordedEvent) -> Result<(), RecordError> {
        let event = &recorded.event;
        event.validate()?;
        if event.patient() != self.patient {
            return Err(RecordError::WrongPatient { stream: self.patient, event: event.patient() });
        }
        match event {
            ClinicalEvent::PatientRegistered { .. } => return Err(RecordError::AlreadyRegistered),
            ClinicalEvent::DemographicsAmended { demographics, .. } => self.demographics = demographics.clone(),
            ClinicalEvent::EncounterOpened { encounter, class, start, reason, .. } => {
                if self.encounters.contains_key(encounter) {
                    return Err(RecordError::DuplicateEncounter(*encounter));
                }
                self.encounters.insert(*encounter, Encounter { id: *encounter, class: *class, start: *start, end: None, reason: reason.clone() });
            }
            ClinicalEvent::EncounterClosed { encounter, end, .. } => match self.encounters.get_mut(encounter) {
                Some(e) if e.end.is_none() => e.end = Some(*end),
                _ => return Err(RecordError::EncounterNotOpen(*encounter)),
            },
            ClinicalEvent::ObservationRecorded { observation, encounter, code, value, effective, .. } => {
                if self.observations.iter().any(|o| o.id == *observation) {
                    return Err(RecordError::DuplicateObservation(*observation));
                }
                if let Some(enc) = encounter
                    && !self.encounters.get(enc).is_some_and(|e| e.end.is_none())
                {
                    return Err(RecordError::EncounterNotOpen(*enc));
                }
                self.observations.push(Observation {
                    id: *observation,
                    encounter: *encounter,
                    code: code.clone(),
                    value: value.clone(),
                    effective: *effective,
                    entered_in_error: None,
                    recorded_by: recorded.actor.id.clone(),
                });
            }
            ClinicalEvent::ObservationEnteredInError { observation, reason, .. } => {
                let Some(o) = self.observations.iter_mut().find(|o| o.id == *observation) else {
                    return Err(RecordError::UnknownObservation(*observation));
                };
                o.entered_in_error = Some(reason.clone());
            }
            ClinicalEvent::ConditionRecorded { condition, code, onset, .. } => {
                if self.conditions.iter().any(|c| c.id == *condition) {
                    return Err(RecordError::DuplicateCondition(*condition));
                }
                self.conditions.push(Condition { id: *condition, code: code.clone(), onset: *onset });
            }
        }
        self.version = self.version.saturating_add(1);
        self.last_updated = recorded.recorded_at;
        Ok(())
    }

    /// Observations that are not marked entered-in-error.
    pub fn active_observations(&self) -> impl Iterator<Item = &Observation> {
        self.observations.iter().filter(|o| o.entered_in_error.is_none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use healthcraft_core::SequentialIds;
    use healthcraft_terminology::CodeSystem;

    fn at(secs: i64) -> Timestamp {
        chrono::DateTime::from_timestamp(secs, 0).unwrap()
    }

    fn demo() -> Demographics {
        Demographics {
            given: vec!["Synthetic".into()],
            family: "Testpatient".into(),
            birth_date: NaiveDate::from_ymd_opt(1980, 1, 1).unwrap(),
            sex: Sex::Female,
            address: None,
        }
    }

    fn rec(seq: u64, event: ClinicalEvent) -> RecordedEvent {
        RecordedEvent { id: EventId(uuid_of(seq)), sequence: seq, recorded_at: at(seq as i64), actor: Actor::system(), event }
    }

    fn uuid_of(n: u64) -> healthcraft_core::Uuid {
        healthcraft_core::Uuid::from_u128(u128::from(n))
    }

    fn a1c(value: f64) -> (Coding, ObservationValue) {
        (Coding::new(CodeSystem::Loinc, "4548-4", Some("Hemoglobin A1c".into())).unwrap(), ObservationValue::Quantity { value, unit: "%".into() })
    }

    #[test]
    fn builds_a_chart_from_events() {
        let ids = SequentialIds::default();
        let p = PatientId::new(&ids);
        let e = EncounterId::new(&ids);
        let o = ObservationId::new(&ids);
        let (code, value) = a1c(7.2);
        let events = vec![
            rec(1, ClinicalEvent::PatientRegistered { patient: p, demographics: demo() }),
            rec(2, ClinicalEvent::EncounterOpened { patient: p, encounter: e, class: EncounterClass::Ambulatory, start: at(2), reason: None }),
            rec(3, ClinicalEvent::ObservationRecorded { patient: p, observation: o, encounter: Some(e), code, value, effective: at(3) }),
            rec(4, ClinicalEvent::EncounterClosed { patient: p, encounter: e, end: at(4) }),
            rec(5, ClinicalEvent::ObservationEnteredInError { patient: p, observation: o, reason: "wrong patient".into() }),
        ];
        let chart = Chart::replay(&events).unwrap();
        assert_eq!(chart.version, 5);
        assert_eq!(chart.demographics.display_name(), "Synthetic Testpatient");
        assert_eq!(chart.encounters[&e].end, Some(at(4)));
        assert_eq!(chart.observations.len(), 1);
        assert_eq!(chart.active_observations().count(), 0);
        assert_eq!(chart.last_updated, at(5));
    }

    #[test]
    fn enforces_record_invariants() {
        let ids = SequentialIds::default();
        let p = PatientId::new(&ids);
        let other = PatientId::new(&ids);
        let e = EncounterId::new(&ids);
        let registered = rec(1, ClinicalEvent::PatientRegistered { patient: p, demographics: demo() });

        assert_eq!(Chart::replay(&[]).unwrap_err(), RecordError::NotRegistered);
        let close = rec(1, ClinicalEvent::EncounterClosed { patient: p, encounter: e, end: at(1) });
        assert_eq!(Chart::replay([&close]).unwrap_err(), RecordError::NotRegistered);

        let mut chart = Chart::start(&registered).unwrap();
        let before = chart.clone();
        assert_eq!(chart.apply(&registered).unwrap_err(), RecordError::AlreadyRegistered);
        assert_eq!(chart.apply(&close).unwrap_err(), RecordError::EncounterNotOpen(e));
        let wrong = rec(2, ClinicalEvent::EncounterClosed { patient: other, encounter: e, end: at(1) });
        assert!(matches!(chart.apply(&wrong).unwrap_err(), RecordError::WrongPatient { .. }));
        let (code, value) = a1c(1.0);
        let into_closed =
            rec(2, ClinicalEvent::ObservationRecorded { patient: p, observation: ObservationId::new(&ids), encounter: Some(e), code, value, effective: at(2) });
        assert_eq!(chart.apply(&into_closed).unwrap_err(), RecordError::EncounterNotOpen(e));
        assert_eq!(chart, before, "a rejected event leaves the chart unchanged");

        let open = rec(2, ClinicalEvent::EncounterOpened { patient: p, encounter: e, class: EncounterClass::Virtual, start: at(2), reason: None });
        chart.apply(&open).unwrap();
        assert_eq!(chart.apply(&open).unwrap_err(), RecordError::DuplicateEncounter(e));
    }

    #[test]
    fn rejects_invalid_values() {
        let ids = SequentialIds::default();
        let p = PatientId::new(&ids);
        let (code, _) = a1c(0.0);
        for value in [f64::NAN, f64::INFINITY] {
            let event = ClinicalEvent::ObservationRecorded {
                patient: p,
                observation: ObservationId::new(&ids),
                encounter: None,
                code: code.clone(),
                value: ObservationValue::Quantity { value, unit: "%".into() },
                effective: at(0),
            };
            assert!(event.validate().is_err());
        }
        let mut d = demo();
        d.family = " ".into();
        assert!(ClinicalEvent::PatientRegistered { patient: p, demographics: d }.validate().is_err());
        let mut d = demo();
        d.given = vec!["x".into(); MAX_ITEMS + 1];
        assert!(d.validate().is_err());
        let amend = ClinicalEvent::DemographicsAmended { patient: p, demographics: demo(), reason: "".into() };
        assert!(amend.validate().is_err(), "an amendment needs a reason");
    }

    #[test]
    fn events_round_trip_through_json() {
        let ids = SequentialIds::default();
        let (code, value) = a1c(6.5);
        let event = ClinicalEvent::ObservationRecorded {
            patient: PatientId::new(&ids),
            observation: ObservationId::new(&ids),
            encounter: None,
            code,
            value,
            effective: at(10),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "observationRecorded");
        assert_eq!(json["value"]["type"], "quantity");
        let back: ClinicalEvent = serde_json::from_value(json).unwrap();
        assert_eq!(back, event);
    }
}
