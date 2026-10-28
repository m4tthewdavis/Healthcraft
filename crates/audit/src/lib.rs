//! The access log.
//!
//! HIPAA's Security Rule requires a record of who accessed protected health information
//! (45 CFR 164.312(b)). Every command the engine runs, including reads and refusals, appends one
//! [`AuditEntry`]. Entries are hash-chained: each one stores the SHA-256 of the previous entry, so
//! [`verify`] detects any entry that was edited, removed or reordered after the fact.
//!
//! The chain proves integrity, not completeness at the tail: someone who can truncate the log can
//! drop its newest entries. Deployments should also ship entries to write-once storage.

use std::sync::{Mutex, PoisonError};

use healthcraft_core::{Actor, Timestamp};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// The `prev_hash` of the first entry.
pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Read,
    Write,
    Export,
    Deidentify,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Success,
    /// The actor's role does not allow the command.
    Denied,
    /// The command was allowed but failed (bad parameters, conflict).
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    /// 1-based position in the log.
    pub sequence: u64,
    pub at: Timestamp,
    pub actor: Actor,
    pub action: Action,
    /// The command id, e.g. `patient.chart`.
    pub command: String,
    /// What was touched, e.g. `Patient/<id>`, or `*` for many records.
    pub resource: String,
    pub outcome: Outcome,
    pub prev_hash: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuditError {
    #[error("audit entry {sequence} was altered, removed or reordered")]
    Tampered { sequence: u64 },
}

/// What a caller supplies; the log fills in the sequence and hashes.
#[derive(Debug, Clone)]
pub struct NewEntry {
    pub at: Timestamp,
    pub actor: Actor,
    pub action: Action,
    pub command: String,
    pub resource: String,
    pub outcome: Outcome,
}

/// An in-memory, append-only log. A durable backend is on the roadmap (docs/roadmap.md).
#[derive(Debug, Default)]
pub struct AuditLog {
    entries: Mutex<Vec<AuditEntry>>,
}

impl AuditLog {
    pub fn append(&self, new: NewEntry) -> AuditEntry {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let prev_hash = entries.last().map_or_else(|| GENESIS.to_owned(), |e| e.hash.clone());
        let sequence = u64::try_from(entries.len()).unwrap_or(u64::MAX).saturating_add(1);
        let mut entry = AuditEntry {
            sequence,
            at: new.at,
            actor: new.actor,
            action: new.action,
            command: new.command,
            resource: new.resource,
            outcome: new.outcome,
            prev_hash,
            hash: String::new(),
        };
        entry.hash = hash_of(&entry);
        entries.push(entry.clone());
        entry
    }

    /// A copy of every entry, oldest first.
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Checks the hash chain of a full log (oldest first).
pub fn verify(entries: &[AuditEntry]) -> Result<(), AuditError> {
    let mut prev = GENESIS.to_owned();
    for (index, entry) in entries.iter().enumerate() {
        let expected_sequence = u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1);
        if entry.sequence != expected_sequence || entry.prev_hash != prev || entry.hash != hash_of(entry) {
            return Err(AuditError::Tampered { sequence: expected_sequence });
        }
        prev.clone_from(&entry.hash);
    }
    Ok(())
}

/// SHA-256 over every field but `hash`, each length-prefixed so no two entries share an encoding.
fn hash_of(entry: &AuditEntry) -> String {
    let mut h = Sha256::new();
    let mut field = |bytes: &[u8]| {
        h.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
        h.update(bytes);
    };
    field(&entry.sequence.to_le_bytes());
    field(entry.at.to_rfc3339().as_bytes());
    field(entry.actor.id.as_bytes());
    field(format!("{:?}", entry.actor.role).as_bytes());
    field(format!("{:?}", entry.action).as_bytes());
    field(entry.command.as_bytes());
    field(entry.resource.as_bytes());
    field(format!("{:?}", entry.outcome).as_bytes());
    field(entry.prev_hash.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use healthcraft_core::Role;

    fn entry(command: &str) -> NewEntry {
        NewEntry {
            at: healthcraft_core::Timestamp::from_timestamp(0, 0).unwrap(),
            actor: Actor::new("dr.lee", Role::Clinician).unwrap(),
            action: Action::Read,
            command: command.into(),
            resource: "Patient/1".into(),
            outcome: Outcome::Success,
        }
    }

    #[test]
    fn chain_verifies() {
        let log = AuditLog::default();
        let first = log.append(entry("patient.chart"));
        let second = log.append(entry("patient.list"));
        assert_eq!(first.prev_hash, GENESIS);
        assert_eq!(second.prev_hash, first.hash);
        assert_eq!(second.sequence, 2);
        assert_eq!(verify(&log.entries()), Ok(()));
        assert_eq!(verify(&[]), Ok(()));
    }

    #[test]
    fn detects_edits_removals_and_reordering() {
        let log = AuditLog::default();
        for c in ["a", "b", "c"] {
            log.append(entry(c));
        }
        let mut edited = log.entries();
        edited[1].resource = "Patient/2".into();
        assert_eq!(verify(&edited), Err(AuditError::Tampered { sequence: 2 }));

        let mut removed = log.entries();
        removed.remove(1);
        assert_eq!(verify(&removed), Err(AuditError::Tampered { sequence: 2 }));

        let mut swapped = log.entries();
        swapped.swap(0, 1);
        assert_eq!(verify(&swapped), Err(AuditError::Tampered { sequence: 1 }));
    }
}
