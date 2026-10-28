//! HealthCraft foundation types shared by every other crate.
//!
//! - typed identifiers ([`PatientId`], [`EncounterId`], …) so an encounter id can never be passed
//!   where a patient id is expected;
//! - [`Actor`] and [`Role`]: who is doing something (every write and read is attributed);
//! - [`Clock`] and [`IdSource`]: time and randomness are injected, so the domain is deterministic in
//!   tests and compiles to wasm;
//! - [`limits`]: caps on input sizes, checked before anything is stored.

use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod limits;

pub use uuid::Uuid;

/// A point in time, always UTC.
pub type Timestamp = DateTime<Utc>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    #[error("{field}: {reason}")]
    Invalid { field: &'static str, reason: String },
}

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new(ids: &dyn IdSource) -> Self {
                Self(ids.next_uuid())
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl std::str::FromStr for $name {
            type Err = CoreError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s.trim())
                    .map(Self)
                    .map_err(|e| CoreError::Invalid { field: stringify!($name), reason: e.to_string() })
            }
        }
    };
}

id_type!(
    /// A patient's record (one event stream per patient).
    PatientId
);
id_type!(
    /// One visit, admission or telehealth session.
    EncounterId
);
id_type!(
    /// One recorded observation (a lab result, a vital sign, a survey answer).
    ObservationId
);
id_type!(
    /// One entry on the problem list.
    ConditionId
);
id_type!(
    /// One signed clinical note.
    NoteId
);
id_type!(
    /// One stored event.
    EventId
);

/// What a person or system is allowed to do. Permissions are decided in the engine
/// (`healthcraft-engine`), never in a UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// Reads and writes charts for care.
    Clinician,
    /// Sees de-identified data only.
    Researcher,
    /// Runs exports and administration.
    Admin,
    /// Imports and interfaces (no human at the keyboard).
    System,
}

/// Who did something. Recorded with every event and every audit entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    /// A stable login or service name, e.g. `dr.lee` or `synthea-import`.
    pub id: String,
    pub role: Role,
}

impl Actor {
    pub fn new(id: impl Into<String>, role: Role) -> Result<Self, CoreError> {
        let id = id.into();
        limits::check_text("actor.id", &id, limits::MAX_SHORT_TEXT)?;
        if id.trim().is_empty() {
            return Err(CoreError::Invalid { field: "actor.id", reason: "must not be empty".into() });
        }
        Ok(Self { id, role })
    }

    /// The actor used for imports and tests.
    pub fn system() -> Self {
        Self { id: "system".into(), role: Role::System }
    }
}

/// Source of the current time.
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

/// The wall clock (native targets only; the browser build injects its own).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

#[cfg(not(target_arch = "wasm32"))]
impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Utc::now()
    }
}

/// A clock that returns a fixed time until it is moved (tests, replays).
#[derive(Debug)]
pub struct FixedClock(Mutex<Timestamp>);

impl FixedClock {
    pub fn new(at: Timestamp) -> Self {
        Self(Mutex::new(at))
    }

    pub fn set(&self, at: Timestamp) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = at;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Source of new unique ids.
pub trait IdSource: Send + Sync {
    fn next_uuid(&self) -> Uuid;
}

/// Random (version 4) UUIDs from the operating system.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct RandomIds;

#[cfg(not(target_arch = "wasm32"))]
impl IdSource for RandomIds {
    fn next_uuid(&self) -> Uuid {
        Uuid::new_v4()
    }
}

/// Predictable ids `00000000-0000-0000-0000-000000000001`, `…02`, … for tests and golden files.
#[derive(Debug, Default)]
pub struct SequentialIds(AtomicU64);

impl IdSource for SequentialIds {
    fn next_uuid(&self) -> Uuid {
        let n = self.0.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        Uuid::from_u128(u128::from(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_ids_are_predictable() {
        let ids = SequentialIds::default();
        assert_eq!(PatientId::new(&ids).to_string(), "00000000-0000-0000-0000-000000000001");
        assert_eq!(EncounterId::new(&ids).to_string(), "00000000-0000-0000-0000-000000000002");
    }

    #[test]
    fn ids_parse_and_reject_garbage() {
        let id: PatientId = "00000000-0000-0000-0000-000000000007".parse().unwrap();
        assert_eq!(id.0.as_u128(), 7);
        assert!("not-a-uuid".parse::<PatientId>().is_err());
        assert!("".parse::<PatientId>().is_err());
    }

    #[test]
    fn ids_serialize_as_plain_strings() {
        let id = PatientId(Uuid::from_u128(1));
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"00000000-0000-0000-0000-000000000001\"");
    }

    #[test]
    fn actor_rejects_empty_and_oversized_ids() {
        assert!(Actor::new("  ", Role::Clinician).is_err());
        assert!(Actor::new("x".repeat(limits::MAX_SHORT_TEXT + 1), Role::Clinician).is_err());
        assert!(Actor::new("dr.lee", Role::Clinician).is_ok());
    }

    #[test]
    fn fixed_clock_moves_only_when_set() {
        let t0 = DateTime::from_timestamp(0, 0).unwrap();
        let t1 = DateTime::from_timestamp(60, 0).unwrap();
        let clock = FixedClock::new(t0);
        assert_eq!(clock.now(), t0);
        clock.set(t1);
        assert_eq!(clock.now(), t1);
    }
}
