//! The FHIR R4 types HealthCraft reads and writes. Fields HealthCraft does not use are ignored on
//! input and omitted on output.

use serde::{Deserialize, Serialize};

/// A resource, tagged by `resourceType`. Types HealthCraft does not handle yet parse as `Other`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "resourceType")]
pub enum Resource {
    Patient(Patient),
    Encounter(Encounter),
    Observation(Observation),
    Condition(Condition),
    Bundle(Bundle),
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Coding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeableConcept {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coding: Vec<Coding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quantity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

impl Reference {
    pub fn to(target: String) -> Self {
        Self { reference: Some(target) }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Period {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanName {
    #[serde(default, rename = "use", skip_serializing_if = "Option::is_none")]
    pub use_: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub given: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Address {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Patient {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub name: Vec<HumanName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_date: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub address: Vec<Address>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Encounter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// In R4 `Encounter.class` is a single Coding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<Coding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<Period>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_code: Vec<CodeableConcept>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default)]
    pub code: CodeableConcept,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter: Option<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_date_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_quantity: Option<Quantity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_string: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_codeable_concept: Option<CodeableConcept>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default)]
    pub code: CodeableConcept,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onset_date_time: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entry: Vec<BundleEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_url: Option<String>,
    pub resource: Resource,
}
