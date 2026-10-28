//! Event storage.
//!
//! [`EventStore`] appends events to a patient's stream and reads them back. Two rules hold for
//! every backend:
//!
//! 1. **Optimistic concurrency.** A writer says which version of the stream it read
//!    (`expected_version`); if someone else appended since, the write fails with
//!    [`StoreError::Conflict`] and the writer re-reads. Two clinicians can never silently overwrite
//!    each other.
//! 2. **Only valid history.** New events are applied to the current [`Chart`] before they are
//!    stored, so the store never holds a stream the record would reject.
//!
//! [`InMemoryStore`] is the reference backend. A PostgreSQL backend is on the roadmap.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use healthcraft_core::{Actor, EventId, IdSource, PatientId, Timestamp};
use healthcraft_record::{Chart, ClinicalEvent, RecordError, RecordedEvent};

/// Most events accepted in one append.
pub const MAX_BATCH: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("stream {patient} is at version {actual}, not {expected}; re-read and retry")]
    Conflict { patient: PatientId, expected: u64, actual: u64 },
    #[error("no record for patient {0}")]
    NotFound(PatientId),
    #[error(transparent)]
    Rejected(#[from] RecordError),
    #[error("append must contain between 1 and {MAX_BATCH} events")]
    BadBatch,
}

pub trait EventStore: Send + Sync {
    /// Appends `events` to `patient`'s stream if it is still at `expected_version` (0 for a new
    /// record). Returns the stored events.
    fn append(
        &self,
        patient: PatientId,
        expected_version: u64,
        actor: &Actor,
        at: Timestamp,
        ids: &dyn IdSource,
        events: Vec<ClinicalEvent>,
    ) -> Result<Vec<RecordedEvent>, StoreError>;

    /// The whole stream, oldest first.
    fn read(&self, patient: PatientId) -> Result<Vec<RecordedEvent>, StoreError>;

    /// Every patient with a record, in id order.
    fn patients(&self) -> Vec<PatientId>;

    /// The patient's current chart.
    fn chart(&self, patient: PatientId) -> Result<Chart, StoreError> {
        Ok(Chart::replay(&self.read(patient)?)?)
    }
}

#[derive(Debug, Default)]
pub struct InMemoryStore {
    streams: Mutex<BTreeMap<PatientId, Vec<RecordedEvent>>>,
}

impl EventStore for InMemoryStore {
    fn append(
        &self,
        patient: PatientId,
        expected_version: u64,
        actor: &Actor,
        at: Timestamp,
        ids: &dyn IdSource,
        events: Vec<ClinicalEvent>,
    ) -> Result<Vec<RecordedEvent>, StoreError> {
        if events.is_empty() || events.len() > MAX_BATCH {
            return Err(StoreError::BadBatch);
        }
        let mut streams = self.streams.lock().unwrap_or_else(PoisonError::into_inner);
        let existing = streams.get(&patient).map(Vec::as_slice).unwrap_or_default();
        let actual = u64::try_from(existing.len()).unwrap_or(u64::MAX);
        if actual != expected_version {
            return Err(StoreError::Conflict { patient, expected: expected_version, actual });
        }

        let mut chart = if existing.is_empty() { None } else { Some(Chart::replay(existing)?) };
        let mut recorded = Vec::with_capacity(events.len());
        for (offset, event) in events.into_iter().enumerate() {
            let sequence = actual.saturating_add(u64::try_from(offset).unwrap_or(u64::MAX)).saturating_add(1);
            let r = RecordedEvent { id: EventId::new(ids), sequence, recorded_at: at, actor: actor.clone(), event };
            if r.event.patient() != patient {
                return Err(RecordError::WrongPatient { stream: patient, event: r.event.patient() }.into());
            }
            match chart.as_mut() {
                Some(c) => c.apply(&r)?,
                None => chart = Some(Chart::start(&r)?),
            }
            recorded.push(r);
        }
        streams.entry(patient).or_default().extend(recorded.iter().cloned());
        Ok(recorded)
    }

    fn read(&self, patient: PatientId) -> Result<Vec<RecordedEvent>, StoreError> {
        let streams = self.streams.lock().unwrap_or_else(PoisonError::into_inner);
        streams.get(&patient).cloned().ok_or(StoreError::NotFound(patient))
    }

    fn patients(&self) -> Vec<PatientId> {
        self.streams.lock().unwrap_or_else(PoisonError::into_inner).keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use healthcraft_core::{EncounterId, SequentialIds};
    use healthcraft_record::{Demographics, EncounterClass, Sex};

    fn at() -> Timestamp {
        Timestamp::from_timestamp(0, 0).unwrap()
    }

    fn register(p: PatientId) -> ClinicalEvent {
        ClinicalEvent::PatientRegistered {
            patient: p,
            demographics: Demographics {
                given: vec!["Synthetic".into()],
                family: "Testpatient".into(),
                birth_date: NaiveDate::from_ymd_opt(1990, 5, 1).unwrap(),
                sex: Sex::Unknown,
                address: None,
            },
        }
    }

    #[test]
    fn appends_and_reads_back() {
        let (store, ids, actor) = (InMemoryStore::default(), SequentialIds::default(), Actor::system());
        let p = PatientId::new(&ids);
        let stored = store.append(p, 0, &actor, at(), &ids, vec![register(p)]).unwrap();
        assert_eq!(stored[0].sequence, 1);
        let e = EncounterId::new(&ids);
        let open = ClinicalEvent::EncounterOpened { patient: p, encounter: e, class: EncounterClass::Home, start: at(), reason: None };
        store.append(p, 1, &actor, at(), &ids, vec![open]).unwrap();
        assert_eq!(store.read(p).unwrap().len(), 2);
        assert_eq!(store.chart(p).unwrap().version, 2);
        assert_eq!(store.patients(), vec![p]);
    }

    #[test]
    fn detects_concurrent_writers() {
        let (store, ids, actor) = (InMemoryStore::default(), SequentialIds::default(), Actor::system());
        let p = PatientId::new(&ids);
        store.append(p, 0, &actor, at(), &ids, vec![register(p)]).unwrap();
        let close = ClinicalEvent::EncounterClosed { patient: p, encounter: EncounterId::new(&ids), end: at() };
        assert_eq!(store.append(p, 0, &actor, at(), &ids, vec![close]).unwrap_err(), StoreError::Conflict { patient: p, expected: 0, actual: 1 });
    }

    #[test]
    fn rejects_invalid_history_atomically() {
        let (store, ids, actor) = (InMemoryStore::default(), SequentialIds::default(), Actor::system());
        let p = PatientId::new(&ids);
        let e = EncounterId::new(&ids);
        let close = ClinicalEvent::EncounterClosed { patient: p, encounter: e, end: at() };
        // A valid first event followed by an invalid one: nothing is stored.
        let err = store.append(p, 0, &actor, at(), &ids, vec![register(p), close]).unwrap_err();
        assert_eq!(err, StoreError::Rejected(RecordError::EncounterNotOpen(e)));
        assert_eq!(store.read(p).unwrap_err(), StoreError::NotFound(p));
        assert_eq!(store.append(p, 0, &actor, at(), &ids, vec![]).unwrap_err(), StoreError::BadBatch);
        let other = PatientId::new(&ids);
        assert!(matches!(store.append(p, 0, &actor, at(), &ids, vec![register(other)]).unwrap_err(), StoreError::Rejected(RecordError::WrongPatient { .. })));
    }
}
