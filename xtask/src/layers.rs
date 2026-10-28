//! Dependency layering rules (docs/architecture.md › Layers).
//!
//! A crate may depend only on crates in strictly lower layers. Standalone crates depend on no
//! workspace crate. UI toolkits and the async/HTTP stack are only allowed at L5 and above, so the
//! domain (L0-L4) stays headless, synchronous and wasm-compatible. The rule engine works on a small
//! model so it can be unit-tested; `from_metadata` builds that model from `cargo metadata`.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Layer(u8),
    /// Layer 0, and additionally depends on no workspace crate (publishable on its own).
    Standalone,
    /// Test helpers: other crates may use it only as a dev-dependency.
    Testkit,
    /// Binaries and tooling: exempt.
    Exempt,
}

/// The layering table, by package name without the `healthcraft-` prefix.
pub const TABLE: &[(&str, Class)] = &[
    ("core", Class::Layer(0)),
    ("terminology", Class::Standalone),
    ("hl7v2", Class::Standalone),
    ("record", Class::Layer(1)),
    ("audit", Class::Layer(1)),
    ("store", Class::Layer(2)),
    ("fhir", Class::Layer(3)),
    ("research", Class::Layer(3)),
    ("engine", Class::Layer(4)),
    ("api", Class::Layer(5)),
    ("ui-egui", Class::Layer(5)),
    ("testkit", Class::Testkit),
    ("healthcraft", Class::Exempt),
    ("server", Class::Exempt),
    ("cli", Class::Exempt),
    ("xtask", Class::Exempt),
];

/// External crates that pull in a UI toolkit, windowing, or the async/HTTP stack. Entries ending in
/// `*` are prefixes.
pub const EDGE_CRATES: &[&str] = &["egui*", "eframe", "winit", "wgpu*", "axum*", "tokio*", "tower*", "hyper*"];

/// First layer allowed to use [`EDGE_CRATES`].
pub const EDGE_MIN_LAYER: u8 = 5;

pub fn short_name(pkg: &str) -> &str {
    pkg.strip_prefix("healthcraft-").unwrap_or(pkg)
}

pub fn classify(pkg: &str) -> Option<Class> {
    let s = short_name(pkg);
    TABLE.iter().find(|(n, _)| *n == s).map(|(_, c)| *c)
}

fn layer(class: Class) -> Option<u8> {
    match class {
        Class::Layer(l) => Some(l),
        Class::Standalone => Some(0),
        Class::Testkit => Some(EDGE_MIN_LAYER),
        Class::Exempt => None,
    }
}

/// The crates `cargo xtask wasm` checks: every layered crate below the edge.
pub fn wasm_crates() -> Vec<&'static str> {
    const NAMES: &[&str] = &[
        "healthcraft-core",
        "healthcraft-terminology",
        "healthcraft-hl7v2",
        "healthcraft-record",
        "healthcraft-audit",
        "healthcraft-store",
        "healthcraft-fhir",
        "healthcraft-research",
        "healthcraft-engine",
    ];
    NAMES.to_vec()
}

fn is_edge_crate(name: &str) -> bool {
    EDGE_CRATES.iter().any(|p| match p.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == *p,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub kind: DepKind,
    pub workspace: bool,
}

#[derive(Debug, Clone)]
pub struct Crate {
    pub name: String,
    pub deps: Vec<Dep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    Unregistered { krate: String },
    Upward { krate: String, dep: String, from: u8, to: u8 },
    StandaloneHasWorkspaceDep { krate: String, dep: String },
    TestkitAsNormalDep { krate: String },
    EdgeBelowLayer { krate: String, dep: String, layer: u8 },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unregistered { krate } => write!(f, "{krate}: unknown workspace crate; register it in xtask/src/layers.rs TABLE"),
            Self::Upward { krate, dep, from, to } => write!(f, "{krate} (L{from}) -> {dep} (L{to}): may only depend on strictly lower layers"),
            Self::StandaloneHasWorkspaceDep { krate, dep } => write!(f, "{krate}: standalone crate must not depend on workspace crate {dep}"),
            Self::TestkitAsNormalDep { krate } => write!(f, "{krate}: healthcraft-testkit may only be a dev-dependency"),
            Self::EdgeBelowLayer { krate, dep, layer } => {
                write!(f, "{krate} (L{layer}) depends on `{dep}`; UI, async and HTTP crates are only allowed in L{EDGE_MIN_LAYER}+")
            }
        }
    }
}

pub fn check(crates: &[Crate]) -> Vec<Violation> {
    let mut out = Vec::new();
    for c in crates {
        let Some(class) = classify(&c.name) else {
            out.push(Violation::Unregistered { krate: c.name.clone() });
            continue;
        };
        let Some(from) = layer(class) else { continue };
        for d in &c.deps {
            if d.name == c.name {
                continue;
            }
            if d.workspace {
                if d.name.ends_with("testkit") {
                    if d.kind != DepKind::Dev {
                        out.push(Violation::TestkitAsNormalDep { krate: c.name.clone() });
                    }
                    continue;
                }
                if class == Class::Standalone {
                    out.push(Violation::StandaloneHasWorkspaceDep { krate: c.name.clone(), dep: d.name.clone() });
                    continue;
                }
                let Some(to) = classify(&d.name).and_then(layer) else {
                    out.push(Violation::Unregistered { krate: d.name.clone() });
                    continue;
                };
                // Dev-dependencies may reach sideways (tests drive higher layers), never upward.
                let ok = if d.kind == DepKind::Dev { to <= from } else { to < from };
                if !ok {
                    out.push(Violation::Upward { krate: c.name.clone(), dep: d.name.clone(), from, to });
                }
            } else if d.kind == DepKind::Normal && from < EDGE_MIN_LAYER && is_edge_crate(&d.name) {
                out.push(Violation::EdgeBelowLayer { krate: c.name.clone(), dep: d.name.clone(), layer: from });
            }
        }
    }
    out.sort_by_key(|v| v.to_string());
    out.dedup();
    out
}

pub fn from_metadata(metadata: &Value) -> Result<Vec<Crate>, String> {
    let packages = metadata.get("packages").and_then(Value::as_array).ok_or("cargo metadata: no packages")?;
    let members: Vec<&str> = packages.iter().filter_map(|p| p.get("name").and_then(Value::as_str)).collect();
    let mut crates = Vec::new();
    for p in packages {
        let name = p.get("name").and_then(Value::as_str).ok_or("cargo metadata: package without a name")?;
        let deps = p
            .get("dependencies")
            .and_then(Value::as_array)
            .map(|deps| {
                deps.iter()
                    .filter_map(|d| {
                        let dep_name = d.get("name").and_then(Value::as_str)?;
                        let kind = match d.get("kind").and_then(Value::as_str) {
                            Some("dev") => DepKind::Dev,
                            Some("build") => DepKind::Build,
                            _ => DepKind::Normal,
                        };
                        Some(Dep { name: dep_name.to_owned(), kind, workspace: members.contains(&dep_name) })
                    })
                    .collect()
            })
            .unwrap_or_default();
        crates.push(Crate { name: name.to_owned(), deps });
    }
    Ok(crates)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn krate(name: &str, deps: &[(&str, DepKind, bool)]) -> Crate {
        Crate { name: name.into(), deps: deps.iter().map(|(n, k, w)| Dep { name: (*n).into(), kind: *k, workspace: *w }).collect() }
    }

    #[test]
    fn accepts_downward_dependencies() {
        let crates = [
            krate("healthcraft-engine", &[("healthcraft-store", DepKind::Normal, true), ("serde", DepKind::Normal, false)]),
            krate("healthcraft-api", &[("healthcraft-engine", DepKind::Normal, true), ("axum", DepKind::Normal, false)]),
            krate("healthcraft-engine", &[("healthcraft-testkit", DepKind::Dev, true)]),
        ];
        assert_eq!(check(&crates), vec![]);
    }

    #[test]
    fn rejects_violations() {
        let crates = [
            krate("healthcraft-store", &[("healthcraft-engine", DepKind::Normal, true)]),
            krate("healthcraft-hl7v2", &[("healthcraft-core", DepKind::Normal, true)]),
            krate("healthcraft-record", &[("tokio", DepKind::Normal, false)]),
            krate("healthcraft-fhir", &[("healthcraft-testkit", DepKind::Normal, true)]),
            krate("healthcraft-mystery", &[]),
        ];
        let v = check(&crates);
        assert_eq!(v.len(), 5, "{v:?}");
    }
}
