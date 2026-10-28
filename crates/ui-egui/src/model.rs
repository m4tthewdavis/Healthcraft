//! The chart as the screens need it, read once from the `patient.chart` reply. Parsing is
//! tolerant: a missing or odd field shows as empty, never as a crash.

use chrono::{Datelike, NaiveDate};
use serde_json::Value;

/// A string at `path` in `value`, or empty.
pub fn text(value: &Value, path: &[&str]) -> String {
    let mut v = value;
    for key in path {
        match v.get(key) {
            Some(next) => v = next,
            None => return String::new(),
        }
    }
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn coding_label(code: &Value) -> String {
    let display = text(code, &["display"]);
    if display.is_empty() { format!("{} {}", text(code, &["system"]), text(code, &["code"])) } else { display }
}

pub fn value_label(value: &Value) -> String {
    match value.get("type").and_then(Value::as_str) {
        Some("quantity") => format!("{} {}", text(value, &["value", "value"]), text(value, &["value", "unit"])).trim().to_owned(),
        Some("coded") => value.get("value").map(coding_label).unwrap_or_default(),
        _ => text(value, &["value"]),
    }
}

/// `YYYY-MM-DD` from an RFC 3339 time (or the text itself if it is shorter).
pub fn day(rfc3339: &str) -> &str {
    rfc3339.get(..10).unwrap_or(rfc3339)
}

/// `HH:MM` from an RFC 3339 time.
pub fn clock(rfc3339: &str) -> &str {
    rfc3339.get(11..16).unwrap_or("")
}

/// A long LOINC display cut at its first bracket or slash ("Hemoglobin A1c/Hemoglobin.total…" →
/// "Hemoglobin A1c").
pub fn short(label: &str) -> String {
    label.split(['[', '/']).next().unwrap_or(label).trim().to_owned()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Encounter {
    pub id: String,
    pub class: String,
    pub reason: String,
    pub start: String,
    pub end: Option<String>,
}

impl Encounter {
    pub fn class_label(&self) -> &str {
        match self.class.as_str() {
            "ambulatory" => "Office visit",
            "emergency" => "Emergency",
            "inpatient" => "Inpatient",
            "virtual" => "Telehealth",
            "home" => "Home visit",
            other => other,
        }
    }

    pub fn title(&self) -> String {
        if self.reason.is_empty() { self.class_label().to_owned() } else { self.reason.clone() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: String,
    pub encounter: Option<String>,
    pub title: String,
    pub text: String,
    pub author: String,
    pub signed: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub encounter: Option<String>,
    pub test: String,
    pub value: String,
    pub number: Option<f64>,
    pub effective: String,
    pub entered_in_error: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub label: String,
    pub onset: String,
}

/// One row of the results flowsheet: a test and its values, newest first.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub test: String,
    pub values: Vec<(String, String, Option<f64>)>,
}

impl Series {
    /// Direction of the latest change: `Some(1)` up, `Some(-1)` down, `Some(0)` unchanged.
    pub fn trend(&self) -> Option<i8> {
        let mut numbers = self.values.iter().filter_map(|(_, _, n)| *n);
        let latest = numbers.next()?;
        let previous = numbers.next()?;
        Some(if latest > previous {
            1
        } else if latest < previous {
            -1
        } else {
            0
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChartModel {
    pub patient_id: String,
    pub name: String,
    pub sex: String,
    pub birth_date: String,
    pub city_state: String,
    pub version: String,
    /// Newest first.
    pub encounters: Vec<Encounter>,
    /// Newest first.
    pub notes: Vec<Note>,
    /// Newest first.
    pub observations: Vec<Observation>,
    pub conditions: Vec<Condition>,
}

impl ChartModel {
    pub fn from_json(chart: &Value) -> Self {
        let d = chart.get("demographics").cloned().unwrap_or_default();
        let given: Vec<String> =
            d.get("given").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default();
        let family = text(&d, &["family"]);
        let city = text(&d, &["address", "city"]);
        let state = text(&d, &["address", "state"]);

        let mut encounters: Vec<Encounter> = chart
            .get("encounters")
            .and_then(Value::as_object)
            .map(|m| {
                m.values()
                    .map(|e| Encounter {
                        id: text(e, &["id"]),
                        class: text(e, &["class"]),
                        reason: text(e, &["reason"]),
                        start: text(e, &["start"]),
                        end: Some(text(e, &["end"])).filter(|s| !s.is_empty()),
                    })
                    .collect()
            })
            .unwrap_or_default();
        encounters.sort_by(|a, b| b.start.cmp(&a.start));

        let mut notes: Vec<Note> = array(chart, "notes")
            .iter()
            .map(|n| Note {
                id: text(n, &["id"]),
                encounter: Some(text(n, &["encounter"])).filter(|s| !s.is_empty()),
                title: text(n, &["title"]),
                text: text(n, &["text"]),
                author: text(n, &["author"]),
                signed: text(n, &["signed"]),
            })
            .collect();
        notes.sort_by(|a, b| b.signed.cmp(&a.signed));

        let mut observations: Vec<Observation> = array(chart, "observations")
            .iter()
            .map(|o| {
                let value = o.get("value").cloned().unwrap_or_default();
                Observation {
                    encounter: Some(text(o, &["encounter"])).filter(|s| !s.is_empty()),
                    test: short(&coding_label(o.get("code").unwrap_or(&Value::Null))),
                    value: value_label(&value),
                    number: value.get("value").and_then(|v| v.get("value")).and_then(Value::as_f64),
                    effective: text(o, &["effective"]),
                    entered_in_error: o.get("enteredInError").is_some_and(|v| !v.is_null()),
                }
            })
            .collect();
        observations.sort_by(|a, b| b.effective.cmp(&a.effective));

        let conditions = array(chart, "conditions")
            .iter()
            .map(|c| Condition { label: coding_label(c.get("code").unwrap_or(&Value::Null)), onset: text(c, &["onset"]) })
            .collect();

        Self {
            patient_id: text(chart, &["patient"]),
            name: format!("{} {family}", given.join(" ")).trim().to_owned(),
            sex: text(&d, &["sex"]),
            birth_date: text(&d, &["birthDate"]),
            city_state: [city, state].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", "),
            version: text(chart, &["version"]),
            encounters,
            notes,
            observations,
            conditions,
        }
    }

    /// Age in whole years on `today`, if the birth date is readable.
    pub fn age_on(&self, today: NaiveDate) -> Option<i32> {
        let birth = NaiveDate::parse_from_str(&self.birth_date, "%Y-%m-%d").ok()?;
        let mut age = today.year().checked_sub(birth.year())?;
        if (today.month(), today.day()) < (birth.month(), birth.day()) {
            age = age.checked_sub(1)?;
        }
        Some(age)
    }

    pub fn encounter(&self, id: &str) -> Option<&Encounter> {
        self.encounters.iter().find(|e| e.id == id)
    }

    /// Results as a flowsheet: one series per test, in first-seen order (newest results first).
    pub fn flowsheet(&self) -> Vec<Series> {
        let mut series: Vec<Series> = Vec::new();
        for o in self.observations.iter().filter(|o| !o.entered_in_error) {
            let entry = (day(&o.effective).to_owned(), o.value.clone(), o.number);
            match series.iter_mut().find(|s| s.test == o.test) {
                Some(s) => s.values.push(entry),
                None => series.push(Series { test: o.test.clone(), values: vec![entry] }),
            }
        }
        series
    }

    /// Distinct result dates, newest first.
    pub fn result_dates(&self) -> Vec<String> {
        let mut dates: Vec<String> = self.observations.iter().filter(|o| !o.entered_in_error).map(|o| day(&o.effective).to_owned()).collect();
        dates.dedup();
        dates
    }
}

fn array(value: &Value, key: &str) -> Vec<Value> {
    value.get(key).and_then(Value::as_array).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_a_chart_and_tolerates_garbage() {
        let chart = json!({
            "patient": "p1",
            "demographics": {"given": ["Ann"], "family": "Example", "birthDate": "1960-05-20", "sex": "female", "address": {"city": "Lowell", "state": "MA"}},
            "encounters": {"e1": {"id": "e1", "class": "virtual", "start": "2025-01-02T10:00:00Z", "end": null}},
            "observations": [
                {"code": {"display": "Hemoglobin A1c/Hemoglobin.total"}, "value": {"type": "quantity", "value": {"value": 7.0, "unit": "%"}}, "effective": "2025-01-02T10:00:00Z"},
                {"code": {"display": "Hemoglobin A1c/Hemoglobin.total"}, "value": {"type": "quantity", "value": {"value": 8.0, "unit": "%"}}, "effective": "2024-06-02T10:00:00Z"}
            ],
            "notes": [{"id": "n1", "title": "Progress note", "text": "S: ok", "author": "dr", "signed": "2025-01-02T10:30:00Z", "encounter": "e1"}],
            "version": 5
        });
        let m = ChartModel::from_json(&chart);
        assert_eq!(m.name, "Ann Example");
        assert_eq!(m.city_state, "Lowell, MA");
        assert_eq!(m.encounters[0].title(), "Telehealth");
        assert_eq!(m.encounters[0].end, None);
        assert_eq!(m.age_on(NaiveDate::from_ymd_opt(2026, 5, 19).unwrap()), Some(65));
        assert_eq!(m.age_on(NaiveDate::from_ymd_opt(2026, 5, 20).unwrap()), Some(66));
        let sheet = m.flowsheet();
        assert_eq!(sheet.len(), 1);
        assert_eq!(sheet[0].test, "Hemoglobin A1c");
        assert_eq!(sheet[0].trend(), Some(-1));
        assert_eq!(m.result_dates(), vec!["2025-01-02", "2024-06-02"]);
        assert_eq!(m.notes[0].encounter.as_deref(), Some("e1"));

        let junk = ChartModel::from_json(&json!({"demographics": 5, "encounters": [1], "observations": {"x": null}, "notes": "no"}));
        assert!(junk.encounters.is_empty() && junk.notes.is_empty() && junk.observations.is_empty());
        assert_eq!(junk.age_on(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()), None);
    }

    #[test]
    fn helpers() {
        assert_eq!(value_label(&json!({"type":"quantity","value":{"value":7.2,"unit":"%"}})), "7.2 %");
        assert_eq!(day("2025"), "2025");
        assert_eq!(clock("2025-01-02T10:05:00Z"), "10:05");
        assert_eq!(clock("2025"), "");
        assert_eq!(short("Cholesterol in LDL [Mass/volume] in Serum"), "Cholesterol in LDL");
    }
}
