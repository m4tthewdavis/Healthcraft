//! The clinician UI, drawn with egui.
//!
//! The UI is a thin client of the engine: everything it shows comes from a command
//! (`patient.list`, `patient.chart`), and everything it changes will go through one. It never
//! reads the store, so what a user can see is exactly what their role allows and every view is
//! in the access log.

use std::sync::Arc;

use egui::{Color32, RichText, Ui};
use healthcraft_core::Actor;
use healthcraft_engine::Session;
use serde_json::{Value, json};

pub mod theme;

use theme::Tokens;

/// One row of the patient list.
#[derive(Debug, Clone, PartialEq)]
pub struct PatientRow {
    pub id: String,
    pub name: String,
    pub birth_date: String,
}

pub struct ChartApp {
    session: Arc<Session>,
    actor: Actor,
    tokens: Tokens,
    patients: Vec<PatientRow>,
    filter: String,
    selected: Option<String>,
    chart: Option<Value>,
    status: Option<String>,
}

impl ChartApp {
    pub fn new(session: Arc<Session>, actor: Actor) -> Self {
        let mut app =
            Self { session, actor, tokens: Tokens::default(), patients: Vec::new(), filter: String::new(), selected: None, chart: None, status: None };
        app.refresh();
        app
    }

    pub fn patients(&self) -> &[PatientRow] {
        &self.patients
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn chart(&self) -> Option<&Value> {
        self.chart.as_ref()
    }

    /// Reloads the patient list (and the open chart).
    pub fn refresh(&mut self) {
        match self.session.dispatch(&self.actor, "patient.list", &Value::Null) {
            Ok(Value::Array(rows)) => {
                self.patients =
                    rows.iter().map(|r| PatientRow { id: text(r, &["patientId"]), name: text(r, &["name"]), birth_date: text(r, &["birthDate"]) }).collect();
                self.status = None;
            }
            Ok(_) => self.status = Some("Unexpected reply from patient.list".into()),
            Err(e) => {
                self.patients.clear();
                self.status = Some(e.to_string());
            }
        }
        if let Some(id) = self.selected.clone() {
            self.select(&id);
        }
    }

    pub fn select(&mut self, patient_id: &str) {
        self.selected = Some(patient_id.to_owned());
        match self.session.dispatch(&self.actor, "patient.chart", &json!({ "patientId": patient_id })) {
            Ok(chart) => {
                self.chart = Some(chart);
                self.status = None;
            }
            Err(e) => {
                self.chart = None;
                self.status = Some(e.to_string());
            }
        }
    }

    /// Draws the whole window into `ui` (the root `Ui` of a frame).
    pub fn show(&mut self, ui: &mut Ui) {
        let t = self.tokens;
        egui::Panel::top("hc_top").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("HealthCraft").color(t.accent));
                ui.separator();
                ui.label(format!("{} ({:?})", self.actor.id, self.actor.role));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Refresh").clicked() {
                        self.refresh();
                    }
                });
            });
            ui.label(RichText::new("Synthetic data only. Not certified health IT; not for clinical use.").color(t.warning));
            if let Some(status) = &self.status {
                ui.label(RichText::new(status).color(t.danger));
            }
        });

        let mut clicked: Option<String> = None;
        egui::Panel::left("hc_patients").resizable(true).default_size(260.0).show(ui, |ui| {
            ui.heading("Patients");
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Search name"));
            ui.separator();
            let needle = self.filter.to_lowercase();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for p in self.patients.iter().filter(|p| needle.is_empty() || p.name.to_lowercase().contains(&needle)) {
                    let selected = self.selected.as_deref() == Some(p.id.as_str());
                    let label = format!("{}\n{}", p.name, p.birth_date);
                    if ui.selectable_label(selected, label).clicked() {
                        clicked = Some(p.id.clone());
                    }
                }
                if self.patients.is_empty() {
                    ui.weak("No patients. Import a FHIR bundle (Synthea) to begin.");
                }
            });
        });
        if let Some(id) = clicked {
            self.select(&id);
        }

        egui::CentralPanel::default_margins().show(ui, |ui| match &self.chart {
            Some(chart) => egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| chart_view(ui, chart, &t)).inner,
            None => {
                ui.weak("Select a patient.");
            }
        });
    }
}

/// A string at `path` in `value`, or empty.
fn text(value: &Value, path: &[&str]) -> String {
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

fn coding_label(code: &Value) -> String {
    let display = text(code, &["display"]);
    if display.is_empty() { format!("{} {}", text(code, &["system"]), text(code, &["code"])) } else { display }
}

fn value_label(value: &Value) -> String {
    match value.get("type").and_then(Value::as_str) {
        Some("quantity") => format!("{} {}", text(value, &["value", "value"]), text(value, &["value", "unit"])).trim().to_owned(),
        Some("coded") => value.get("value").map(coding_label).unwrap_or_default(),
        _ => text(value, &["value"]),
    }
}

fn day(rfc3339: &str) -> &str {
    rfc3339.get(..10).unwrap_or(rfc3339)
}

fn chart_view(ui: &mut Ui, chart: &Value, t: &Tokens) {
    let d = chart.get("demographics").cloned().unwrap_or_default();
    let given: Vec<String> =
        d.get("given").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default();
    ui.heading(format!("{} {}", given.join(" "), text(&d, &["family"])).trim().to_owned());
    ui.label(format!("Born {} · {} · record version {}", text(&d, &["birthDate"]), text(&d, &["sex"]), text(chart, &["version"])));
    ui.add_space(8.0);

    ui.label(RichText::new("Problem list").strong());
    let conditions = chart.get("conditions").and_then(Value::as_array).cloned().unwrap_or_default();
    if conditions.is_empty() {
        ui.weak("None recorded");
    }
    for c in &conditions {
        let onset = text(c, &["onset"]);
        ui.label(if onset.is_empty() {
            coding_label(c.get("code").unwrap_or(&Value::Null))
        } else {
            format!("{} (since {onset})", coding_label(c.get("code").unwrap_or(&Value::Null)))
        });
    }
    ui.add_space(8.0);

    ui.label(RichText::new("Encounters").strong());
    let encounters: Vec<Value> = chart.get("encounters").and_then(Value::as_object).map(|m| m.values().cloned().collect()).unwrap_or_default();
    egui::Grid::new("hc_encounters").num_columns(3).striped(true).show(ui, |ui| {
        for e in &encounters {
            ui.label(day(&text(e, &["start"])));
            ui.label(text(e, &["class"]));
            ui.label(text(e, &["reason"]));
            ui.end_row();
        }
    });
    ui.add_space(8.0);

    ui.label(RichText::new("Results and observations").strong());
    let observations = chart.get("observations").and_then(Value::as_array).cloned().unwrap_or_default();
    egui::Grid::new("hc_observations").num_columns(3).striped(true).show(ui, |ui| {
        for o in observations.iter().rev() {
            let in_error = o.get("enteredInError").is_some_and(|v| !v.is_null());
            let style = |s: String| if in_error { RichText::new(s).strikethrough().color(t.muted) } else { RichText::new(s) };
            ui.label(style(day(&text(o, &["effective"])).to_owned()));
            ui.label(style(coding_label(o.get("code").unwrap_or(&Value::Null))));
            ui.label(style(value_label(o.get("value").unwrap_or(&Value::Null))));
            ui.end_row();
        }
    });
}

/// A small palette check used by tests and the theme: every token is opaque.
pub fn is_opaque(c: Color32) -> bool {
    c.a() == 255
}

#[cfg(test)]
mod tests {
    use super::*;
    use healthcraft_core::Role;
    use healthcraft_testkit::{SYNTHETIC_BUNDLE, SYNTHETIC_PATIENT_ID, actor, session};

    fn loaded() -> Arc<Session> {
        let s = session();
        s.dispatch(&actor(Role::System), "import.fhirBundle", &json!({ "bundle": SYNTHETIC_BUNDLE })).unwrap();
        Arc::new(s)
    }

    fn frame(app: &mut ChartApp) {
        let ctx = egui::Context::default();
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), ..Default::default() };
        for _ in 0..2 {
            ctx.run_ui(input.clone(), |ui| app.show(ui)).drop_without_applying_deltas();
        }
    }

    #[test]
    fn clinician_sees_the_list_and_the_chart() {
        let mut app = ChartApp::new(loaded(), actor(Role::Clinician));
        assert_eq!(app.patients().len(), 1);
        assert_eq!(app.patients()[0].name, "Synthetic Testpatient");
        app.select(SYNTHETIC_PATIENT_ID);
        assert_eq!(app.chart().unwrap()["demographics"]["family"], "Testpatient");
        frame(&mut app);
        assert_eq!(app.status(), None);
    }

    #[test]
    fn researcher_is_refused_identified_data() {
        let mut app = ChartApp::new(loaded(), actor(Role::Researcher));
        assert!(app.patients().is_empty());
        assert!(app.status().unwrap().contains("may not run"));
        app.select(SYNTHETIC_PATIENT_ID);
        assert!(app.chart().is_none());
        frame(&mut app);
    }

    #[test]
    fn renders_empty_and_odd_charts_without_panicking() {
        let mut app = ChartApp::new(Arc::new(session()), actor(Role::Clinician));
        frame(&mut app);
        app.chart = Some(json!({"demographics": 5, "encounters": [1], "observations": {"x": null}, "conditions": "no"}));
        frame(&mut app);
        assert_eq!(value_label(&json!({"type":"quantity","value":{"value":7.2,"unit":"%"}})), "7.2 %");
        assert_eq!(day("2025"), "2025");
    }

    #[test]
    fn theme_tokens_are_opaque() {
        let t = Tokens::default();
        for c in [t.accent, t.warning, t.danger, t.muted] {
            assert!(is_opaque(c));
        }
    }
}
