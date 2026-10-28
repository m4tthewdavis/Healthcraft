//! Test helpers. Used only as a dev-dependency (`cargo xtask layers` enforces it).

use std::sync::Arc;

use healthcraft_core::{Actor, FixedClock, Role, SequentialIds, Timestamp};
use healthcraft_engine::Session;
use healthcraft_store::InMemoryStore;

/// The synthetic FHIR bundle in `fixtures/` (one patient, see `fixtures/README.md`).
pub const SYNTHETIC_BUNDLE: &str = include_str!("../../../fixtures/synthetic-bundle.json");
/// The synthetic HL7 v2 ORU^R01 lab message in `fixtures/`.
pub const SYNTHETIC_ORU: &str = include_str!("../../../fixtures/synthetic-oru.hl7");
/// The patient id both fixtures use.
pub const SYNTHETIC_PATIENT_ID: &str = "8f1d7c2e-0000-4000-8000-000000000001";

/// 2026-10-27T12:00:00Z.
pub fn fixed_time() -> Timestamp {
    Timestamp::from_timestamp(1_793_102_400, 0).unwrap_or_default()
}

/// A session with an in-memory store, a fixed clock and sequential ids, so results are exact.
pub fn session() -> Session {
    Session::new(Arc::new(InMemoryStore::default()), Arc::new(FixedClock::new(fixed_time())), Arc::new(SequentialIds::default()))
}

pub fn actor(role: Role) -> Actor {
    let id = match role {
        Role::Clinician => "dr.test",
        Role::Researcher => "researcher.test",
        Role::Admin => "admin.test",
        Role::System => "system",
    };
    Actor { id: id.into(), role }
}
