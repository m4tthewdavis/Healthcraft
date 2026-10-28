//! OMOP Common Data Model v5.4 rows.
//!
//! Source codes are kept in the `*_source_value` columns. Standard concept ids need the OHDSI
//! vocabulary (downloaded from Athena under its own licenses, like the terminologies in `NOTICE`),
//! so they are 0 ("no matching concept") except for the handful of fixed ids below, until a
//! deployment loads that vocabulary.

use chrono::{Datelike, NaiveDate};
use healthcraft_record::{Chart, EncounterClass, ObservationValue, Sex};
use serde::{Deserialize, Serialize};

use crate::csv::{self, CsvRow};

/// OMOP standard concepts for gender.
pub const MALE: i64 = 8507;
pub const FEMALE: i64 = 8532;
/// OMOP standard concepts for visit types.
pub const INPATIENT_VISIT: i64 = 9201;
pub const OUTPATIENT_VISIT: i64 = 9202;
pub const EMERGENCY_ROOM_VISIT: i64 = 9203;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub person_id: i64,
    pub gender_concept_id: i64,
    pub year_of_birth: i32,
    pub month_of_birth: u32,
    pub day_of_birth: u32,
    pub person_source_value: String,
    pub gender_source_value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisitOccurrence {
    pub visit_occurrence_id: i64,
    pub person_id: i64,
    pub visit_concept_id: i64,
    pub visit_start_date: NaiveDate,
    pub visit_end_date: NaiveDate,
    pub visit_source_value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    pub measurement_id: i64,
    pub person_id: i64,
    pub measurement_concept_id: i64,
    pub measurement_date: NaiveDate,
    pub value_as_number: f64,
    pub unit_source_value: String,
    pub measurement_source_value: String,
    pub visit_occurrence_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub observation_id: i64,
    pub person_id: i64,
    pub observation_concept_id: i64,
    pub observation_date: NaiveDate,
    pub value_as_string: String,
    pub observation_source_value: String,
    pub visit_occurrence_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionOccurrence {
    pub condition_occurrence_id: i64,
    pub person_id: i64,
    pub condition_concept_id: i64,
    pub condition_start_date: NaiveDate,
    pub condition_source_value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Tables {
    pub person: Vec<Person>,
    pub visit_occurrence: Vec<VisitOccurrence>,
    pub measurement: Vec<Measurement>,
    pub observation: Vec<Observation>,
    pub condition_occurrence: Vec<ConditionOccurrence>,
    /// Facts that have no OMOP row (a condition without an onset date has no required start date).
    pub not_exported: u64,
}

/// Projects charts into OMOP rows. Row ids are assigned in order, starting at 1 in each table.
/// Observations marked entered-in-error are left out.
pub fn export(charts: &[Chart]) -> Tables {
    let mut t = Tables::default();
    let mut next = Counters::default();
    for chart in charts {
        let person_id = next.person();
        let d = &chart.demographics;
        t.person.push(Person {
            person_id,
            gender_concept_id: match d.sex {
                Sex::Male => MALE,
                Sex::Female => FEMALE,
                Sex::Other | Sex::Unknown => 0,
            },
            year_of_birth: d.birth_date.year(),
            month_of_birth: d.birth_date.month(),
            day_of_birth: d.birth_date.day(),
            person_source_value: chart.patient.to_string(),
            gender_source_value: format!("{:?}", d.sex).to_lowercase(),
        });

        let mut visit_ids = std::collections::BTreeMap::new();
        for e in chart.encounters.values() {
            let id = next.visit();
            visit_ids.insert(e.id, id);
            t.visit_occurrence.push(VisitOccurrence {
                visit_occurrence_id: id,
                person_id,
                visit_concept_id: match e.class {
                    EncounterClass::Inpatient => INPATIENT_VISIT,
                    EncounterClass::Ambulatory => OUTPATIENT_VISIT,
                    EncounterClass::Emergency => EMERGENCY_ROOM_VISIT,
                    // Mapped once the OHDSI vocabulary is loaded.
                    EncounterClass::Virtual | EncounterClass::Home => 0,
                },
                visit_start_date: e.start.date_naive(),
                visit_end_date: e.end.unwrap_or(e.start).date_naive(),
                visit_source_value: format!("{:?}", e.class).to_lowercase(),
            });
        }

        for o in chart.active_observations() {
            let visit_occurrence_id = o.encounter.and_then(|e| visit_ids.get(&e).copied());
            let source = format!("{}|{}", o.code.system.name(), o.code.code);
            match &o.value {
                ObservationValue::Quantity { value, unit } => t.measurement.push(Measurement {
                    measurement_id: next.measurement(),
                    person_id,
                    measurement_concept_id: 0,
                    measurement_date: o.effective.date_naive(),
                    value_as_number: *value,
                    unit_source_value: unit.clone(),
                    measurement_source_value: source,
                    visit_occurrence_id,
                }),
                other => t.observation.push(Observation {
                    observation_id: next.observation(),
                    person_id,
                    observation_concept_id: 0,
                    observation_date: o.effective.date_naive(),
                    value_as_string: other.display(),
                    observation_source_value: source,
                    visit_occurrence_id,
                }),
            }
        }

        for c in &chart.conditions {
            let Some(onset) = c.onset else {
                t.not_exported = t.not_exported.saturating_add(1);
                continue;
            };
            t.condition_occurrence.push(ConditionOccurrence {
                condition_occurrence_id: next.condition(),
                person_id,
                condition_concept_id: 0,
                condition_start_date: onset,
                condition_source_value: format!("{}|{}", c.code.system.name(), c.code.code),
            });
        }
    }
    t
}

#[derive(Default)]
struct Counters {
    person: i64,
    visit: i64,
    measurement: i64,
    observation: i64,
    condition: i64,
}

impl Counters {
    fn bump(n: &mut i64) -> i64 {
        *n = n.saturating_add(1);
        *n
    }
    fn person(&mut self) -> i64 {
        Self::bump(&mut self.person)
    }
    fn visit(&mut self) -> i64 {
        Self::bump(&mut self.visit)
    }
    fn measurement(&mut self) -> i64 {
        Self::bump(&mut self.measurement)
    }
    fn observation(&mut self) -> i64 {
        Self::bump(&mut self.observation)
    }
    fn condition(&mut self) -> i64 {
        Self::bump(&mut self.condition)
    }
}

impl CsvRow for Person {
    const HEADER: &'static [&'static str] =
        &["person_id", "gender_concept_id", "year_of_birth", "month_of_birth", "day_of_birth", "person_source_value", "gender_source_value"];
    fn cells(&self) -> Vec<String> {
        vec![
            self.person_id.to_string(),
            self.gender_concept_id.to_string(),
            self.year_of_birth.to_string(),
            self.month_of_birth.to_string(),
            self.day_of_birth.to_string(),
            csv::text(&self.person_source_value),
            csv::text(&self.gender_source_value),
        ]
    }
}

impl CsvRow for VisitOccurrence {
    const HEADER: &'static [&'static str] =
        &["visit_occurrence_id", "person_id", "visit_concept_id", "visit_start_date", "visit_end_date", "visit_source_value"];
    fn cells(&self) -> Vec<String> {
        vec![
            self.visit_occurrence_id.to_string(),
            self.person_id.to_string(),
            self.visit_concept_id.to_string(),
            self.visit_start_date.to_string(),
            self.visit_end_date.to_string(),
            csv::text(&self.visit_source_value),
        ]
    }
}

impl CsvRow for Measurement {
    const HEADER: &'static [&'static str] = &[
        "measurement_id",
        "person_id",
        "measurement_concept_id",
        "measurement_date",
        "value_as_number",
        "unit_source_value",
        "measurement_source_value",
        "visit_occurrence_id",
    ];
    fn cells(&self) -> Vec<String> {
        vec![
            self.measurement_id.to_string(),
            self.person_id.to_string(),
            self.measurement_concept_id.to_string(),
            self.measurement_date.to_string(),
            self.value_as_number.to_string(),
            csv::text(&self.unit_source_value),
            csv::text(&self.measurement_source_value),
            csv::number(self.visit_occurrence_id),
        ]
    }
}

impl CsvRow for Observation {
    const HEADER: &'static [&'static str] =
        &["observation_id", "person_id", "observation_concept_id", "observation_date", "value_as_string", "observation_source_value", "visit_occurrence_id"];
    fn cells(&self) -> Vec<String> {
        vec![
            self.observation_id.to_string(),
            self.person_id.to_string(),
            self.observation_concept_id.to_string(),
            self.observation_date.to_string(),
            csv::text(&self.value_as_string),
            csv::text(&self.observation_source_value),
            csv::number(self.visit_occurrence_id),
        ]
    }
}

impl CsvRow for ConditionOccurrence {
    const HEADER: &'static [&'static str] = &["condition_occurrence_id", "person_id", "condition_concept_id", "condition_start_date", "condition_source_value"];
    fn cells(&self) -> Vec<String> {
        vec![
            self.condition_occurrence_id.to_string(),
            self.person_id.to_string(),
            self.condition_concept_id.to_string(),
            self.condition_start_date.to_string(),
            csv::text(&self.condition_source_value),
        ]
    }
}

impl Tables {
    /// Each table as `(file name, CSV text)`.
    pub fn to_csv_files(&self) -> Vec<(&'static str, String)> {
        vec![
            ("person.csv", csv::write(&self.person)),
            ("visit_occurrence.csv", csv::write(&self.visit_occurrence)),
            ("measurement.csv", csv::write(&self.measurement)),
            ("observation.csv", csv::write(&self.observation)),
            ("condition_occurrence.csv", csv::write(&self.condition_occurrence)),
        ]
    }
}
