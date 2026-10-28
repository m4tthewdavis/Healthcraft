//! Clinical code systems, codings and deployment-time terminology lookup.
//!
//! Standalone: depends on no other HealthCraft crate. No terminology content is bundled (see the
//! crate README and `NOTICE`).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Longest code or system URI accepted.
pub const MAX_CODE_LEN: usize = 256;
/// Longest display text accepted.
pub const MAX_DISPLAY_LEN: usize = 1024;
/// Most concepts one loaded file may add (larger than SNOMED CT's ~360k active concepts).
pub const MAX_CONCEPTS: usize = 2_000_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TerminologyError {
    #[error("invalid {field}: {reason}")]
    Invalid { field: &'static str, reason: String },
    #[error("line {line}: {reason}")]
    Load { line: usize, reason: String },
}

/// A code system, identified by its canonical FHIR URI.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum CodeSystem {
    Loinc,
    SnomedCt,
    RxNorm,
    Icd10Cm,
    Cpt,
    /// Any other system, by URI (local codes, other national code sets).
    Other(String),
}

impl CodeSystem {
    pub fn uri(&self) -> &str {
        match self {
            Self::Loinc => "http://loinc.org",
            Self::SnomedCt => "http://snomed.info/sct",
            Self::RxNorm => "http://www.nlm.nih.gov/research/umls/rxnorm",
            Self::Icd10Cm => "http://hl7.org/fhir/sid/icd-10-cm",
            Self::Cpt => "http://www.ama-assn.org/go/cpt",
            Self::Other(uri) => uri,
        }
    }

    pub fn from_uri(uri: &str) -> Result<Self, TerminologyError> {
        let uri = uri.trim();
        check("system", uri, MAX_CODE_LEN)?;
        Ok(match uri {
            "http://loinc.org" => Self::Loinc,
            "http://snomed.info/sct" => Self::SnomedCt,
            "http://www.nlm.nih.gov/research/umls/rxnorm" => Self::RxNorm,
            "http://hl7.org/fhir/sid/icd-10-cm" => Self::Icd10Cm,
            "http://www.ama-assn.org/go/cpt" => Self::Cpt,
            other => Self::Other(other.to_owned()),
        })
    }

    /// Whether using the system's content needs a license from its owner (beyond attribution).
    pub fn requires_license(&self) -> bool {
        matches!(self, Self::SnomedCt | Self::Cpt)
    }

    /// A short human name.
    pub fn name(&self) -> &str {
        match self {
            Self::Loinc => "LOINC",
            Self::SnomedCt => "SNOMED CT",
            Self::RxNorm => "RxNorm",
            Self::Icd10Cm => "ICD-10-CM",
            Self::Cpt => "CPT",
            Self::Other(uri) => uri,
        }
    }
}

impl fmt::Display for CodeSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl From<CodeSystem> for String {
    fn from(system: CodeSystem) -> Self {
        system.uri().to_owned()
    }
}

impl TryFrom<String> for CodeSystem {
    type Error = TerminologyError;
    fn try_from(uri: String) -> Result<Self, Self::Error> {
        Self::from_uri(&uri)
    }
}

/// One code from one system, with an optional display text as recorded at the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coding {
    pub system: CodeSystem,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

impl Coding {
    pub fn new(system: CodeSystem, code: impl Into<String>, display: Option<String>) -> Result<Self, TerminologyError> {
        let coding = Self { system, code: code.into(), display };
        coding.validate()?;
        Ok(coding)
    }

    pub fn validate(&self) -> Result<(), TerminologyError> {
        if self.code.trim().is_empty() {
            return Err(TerminologyError::Invalid { field: "code", reason: "must not be empty".into() });
        }
        check("code", &self.code, MAX_CODE_LEN)?;
        check("system", self.system.uri(), MAX_CODE_LEN)?;
        if let Some(display) = &self.display {
            check("display", display, MAX_DISPLAY_LEN)?;
        }
        Ok(())
    }

    /// The display text, else `SYSTEM code`.
    pub fn label(&self) -> String {
        match &self.display {
            Some(d) if !d.trim().is_empty() => d.clone(),
            _ => format!("{} {}", self.system, self.code),
        }
    }
}

fn check(field: &'static str, value: &str, max: usize) -> Result<(), TerminologyError> {
    if value.len() > max {
        return Err(TerminologyError::Invalid { field, reason: format!("longer than {max} bytes") });
    }
    if value.chars().any(char::is_control) {
        return Err(TerminologyError::Invalid { field, reason: "contains control characters".into() });
    }
    Ok(())
}

/// Looks codes up in whatever terminology a deployment has loaded.
pub trait Terminology: Send + Sync {
    /// The preferred display for a code, if the code is known.
    fn display(&self, system: &CodeSystem, code: &str) -> Option<&str>;
}

/// A terminology held in memory, filled from files at startup.
#[derive(Debug, Default, Clone)]
pub struct InMemoryTerminology {
    concepts: BTreeMap<(CodeSystem, String), String>,
}

impl InMemoryTerminology {
    pub fn len(&self) -> usize {
        self.concepts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.concepts.is_empty()
    }

    pub fn insert(&mut self, system: CodeSystem, code: &str, display: &str) -> Result<(), TerminologyError> {
        let coding = Coding::new(system, code.trim(), Some(display.trim().to_owned()))?;
        if self.concepts.len() >= MAX_CONCEPTS {
            return Err(TerminologyError::Invalid { field: "concepts", reason: format!("more than {MAX_CONCEPTS}") });
        }
        self.concepts.insert((coding.system, coding.code), coding.display.unwrap_or_default());
        Ok(())
    }

    /// Loads `code<TAB>display` lines into `system`. Blank lines and lines starting with `#` are
    /// skipped. Returns how many concepts were added.
    pub fn load_tsv(&mut self, system: &CodeSystem, text: &str) -> Result<usize, TerminologyError> {
        let mut added = 0usize;
        for (index, line) in text.lines().enumerate() {
            let line_no = index.saturating_add(1);
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((code, display)) = line.split_once('\t') else {
                return Err(TerminologyError::Load { line: line_no, reason: "expected code<TAB>display".into() });
            };
            self.insert(system.clone(), code, display).map_err(|e| TerminologyError::Load { line: line_no, reason: e.to_string() })?;
            added = added.saturating_add(1);
        }
        Ok(added)
    }
}

impl Terminology for InMemoryTerminology {
    fn display(&self, system: &CodeSystem, code: &str) -> Option<&str> {
        self.concepts.get(&(system.clone(), code.to_owned())).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systems_round_trip_through_their_uris() {
        for system in [CodeSystem::Loinc, CodeSystem::SnomedCt, CodeSystem::RxNorm, CodeSystem::Icd10Cm, CodeSystem::Cpt] {
            assert_eq!(CodeSystem::from_uri(system.uri()).unwrap(), system);
        }
        assert_eq!(CodeSystem::from_uri("urn:local").unwrap(), CodeSystem::Other("urn:local".into()));
        assert!(CodeSystem::from_uri("bad\u{0}uri").is_err());
    }

    #[test]
    fn coding_serializes_system_as_uri() {
        let coding = Coding::new(CodeSystem::Loinc, "4548-4", Some("Hemoglobin A1c".into())).unwrap();
        let json = serde_json::to_string(&coding).unwrap();
        assert_eq!(json, r#"{"system":"http://loinc.org","code":"4548-4","display":"Hemoglobin A1c"}"#);
        let back: Coding = serde_json::from_str(&json).unwrap();
        assert_eq!(back, coding);
    }

    #[test]
    fn coding_validation() {
        assert!(Coding::new(CodeSystem::Loinc, "  ", None).is_err());
        assert!(Coding::new(CodeSystem::Loinc, "x".repeat(MAX_CODE_LEN + 1), None).is_err());
        assert!(Coding::new(CodeSystem::Loinc, "1-8", Some("x".repeat(MAX_DISPLAY_LEN + 1))).is_err());
    }

    #[test]
    fn licensed_systems_are_flagged() {
        assert!(CodeSystem::Cpt.requires_license());
        assert!(CodeSystem::SnomedCt.requires_license());
        assert!(!CodeSystem::Loinc.requires_license());
    }

    #[test]
    fn tsv_loading() {
        let mut t = InMemoryTerminology::default();
        let n = t.load_tsv(&CodeSystem::Loinc, "# comment\n4548-4\tHemoglobin A1c\r\n\n8302-2\tBody height\n").unwrap();
        assert_eq!(n, 2);
        assert_eq!(t.display(&CodeSystem::Loinc, "4548-4"), Some("Hemoglobin A1c"));
        assert_eq!(t.display(&CodeSystem::SnomedCt, "4548-4"), None);
        let err = t.load_tsv(&CodeSystem::Loinc, "ok\tfine\nmissing-tab\n").unwrap_err();
        assert_eq!(err, TerminologyError::Load { line: 2, reason: "expected code<TAB>display".into() });
    }
}
