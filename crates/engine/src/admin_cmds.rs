//! Metadata and the access log.

use healthcraft_core::Actor;
use serde_json::{Value, json};

use crate::{Access, CommandSpec, EngineError, Session, registry, to_json};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec { id: "commands.list", label: "List commands", params: "{}", access: Access::Meta, run: list },
        CommandSpec { id: "audit.entries", label: "Access log", params: "{}", access: Access::Audit, run: entries },
        CommandSpec { id: "audit.verify", label: "Verify access log", params: "{}", access: Access::Audit, run: verify },
    ]
}

fn list(_: &Session, actor: &Actor, _: &Value) -> Result<Value, EngineError> {
    Ok(Value::Array(
        registry()
            .iter()
            .map(|s| json!({ "id": s.id, "label": s.label, "params": s.params, "access": s.access, "allowed": crate::allowed(actor.role, s.access) }))
            .collect(),
    ))
}

fn entries(session: &Session, _: &Actor, _: &Value) -> Result<Value, EngineError> {
    to_json(&session.audit_log().entries())
}

fn verify(session: &Session, _: &Actor, _: &Value) -> Result<Value, EngineError> {
    let entries = session.audit_log().entries();
    healthcraft_audit::verify(&entries).map_err(|e| EngineError::Failed(e.to_string()))?;
    Ok(json!({ "ok": true, "entries": entries.len() }))
}
