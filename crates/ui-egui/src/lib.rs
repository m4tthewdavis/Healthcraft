//! The clinician UI, drawn with egui.
//!
//! The UI is a thin client of the engine: everything it shows comes from a command
//! (`patient.list`, `patient.chart`), and everything it changes will go through one. It never
//! reads the store, so what a user can see is exactly what their role allows and every view is
//! in the access log.

use std::sync::Arc;

use egui::{Color32, RichText, Ui};
use healthcraft_core::{Actor, Role};
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

/// The three things the window can show. Each one is backed by one engine command, so switching
/// to a view the role may not use shows the engine's refusal (and the refusal is in the access log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// `patient.list` + `patient.chart`: identified data for care.
    Chart,
    /// `research.deidentify`: the Safe Harbor data set.
    Research,
    /// `audit.entries`: who did what.
    AccessLog,
}

impl View {
    pub const ALL: [View; 3] = [View::Chart, View::Research, View::AccessLog];

    pub fn label(self) -> &'static str {
        match self {
            View::Chart => "Chart",
            View::Research => "Research data",
            View::AccessLog => "Access log",
        }
    }
}

/// Roles the demo switcher offers.
const DEMO_ROLES: [Role; 3] = [Role::Clinician, Role::Researcher, Role::Admin];

pub struct ChartApp {
    session: Arc<Session>,
    actor: Actor,
    tokens: Tokens,
    view: View,
    role_switcher: bool,
    patients: Vec<PatientRow>,
    filter: String,
    selected: Option<String>,
    chart: Option<Value>,
    research: Option<Value>,
    access_log: Option<Value>,
    status: Option<String>,
}

impl ChartApp {
    pub fn new(session: Arc<Session>, actor: Actor) -> Self {
        let mut app = Self {
            session,
            actor,
            tokens: Tokens::default(),
            view: View::Chart,
            role_switcher: false,
            patients: Vec::new(),
            filter: String::new(),
            selected: None,
            chart: None,
            research: None,
            access_log: None,
            status: None,
        };
        app.refresh();
        app
    }

    /// Shows a role switcher in the top bar, so a demo can show what each role may see. A real
    /// deployment signs people in instead.
    pub fn with_role_switcher(mut self) -> Self {
        self.role_switcher = true;
        self
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

    pub fn view(&self) -> View {
        self.view
    }

    pub fn actor(&self) -> &Actor {
        &self.actor
    }

    /// Acts as someone else from now on and reloads the current view.
    pub fn set_role(&mut self, role: Role) {
        let id = match role {
            Role::Clinician => "dr.demo",
            Role::Researcher => "researcher.demo",
            Role::Admin => "admin.demo",
            Role::System => "system",
        };
        self.actor = Actor { id: id.into(), role };
        self.refresh();
    }

    pub fn set_view(&mut self, view: View) {
        self.view = view;
        self.refresh();
    }

    /// Reloads what the current view shows.
    pub fn refresh(&mut self) {
        self.status = None;
        match self.view {
            View::Chart => self.load_chart_view(),
            View::Research => self.research = self.load("research.deidentify", &Value::Null),
            View::AccessLog => self.access_log = self.load("audit.entries", &Value::Null),
        }
    }

    fn load(&mut self, command: &str, params: &Value) -> Option<Value> {
        match self.session.dispatch(&self.actor, command, params) {
            Ok(v) => Some(v),
            Err(e) => {
                self.status = Some(e.to_string());
                None
            }
        }
    }

    fn load_chart_view(&mut self) {
        match self.load("patient.list", &Value::Null) {
            Some(Value::Array(rows)) => {
                self.patients =
                    rows.iter().map(|r| PatientRow { id: text(r, &["patientId"]), name: text(r, &["name"]), birth_date: text(r, &["birthDate"]) }).collect();
            }
            Some(_) => self.status = Some("Unexpected reply from patient.list".into()),
            None => {
                self.patients.clear();
                self.chart = None;
                return;
            }
        }
        if let Some(id) = self.selected.clone() {
            self.select(&id);
        }
    }

    pub fn select(&mut self, patient_id: &str) {
        self.selected = Some(patient_id.to_owned());
        self.chart = self.load("patient.chart", &json!({ "patientId": patient_id }));
    }

    /// Draws the whole window into `ui` (the root `Ui` of a frame).
    pub fn show(&mut self, ui: &mut Ui) {
        let t = self.tokens;
        let mut new_role = None;
        let mut new_view = None;
        let mut reload = false;
        egui::Panel::top("hc_top").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.heading(RichText::new("HealthCraft").color(t.accent).strong());
                ui.separator();
                if self.role_switcher {
                    ui.label("Signed in as");
                    for role in DEMO_ROLES {
                        if ui.selectable_label(self.actor.role == role, format!("{role:?}")).clicked() && self.actor.role != role {
                            new_role = Some(role);
                        }
                    }
                } else {
                    ui.label(format!("{} ({:?})", self.actor.id, self.actor.role));
                }
                ui.separator();
                for view in View::ALL {
                    if ui.selectable_label(self.view == view, view.label()).clicked() && self.view != view {
                        new_view = Some(view);
                    }
                }
                ui.separator();
                if ui.button("Refresh").clicked() {
                    reload = true;
                }
            });
            ui.label(RichText::new("Synthetic data only. Not certified health IT; not for clinical use.").color(t.warning));
            if let Some(status) = &self.status {
                ui.label(RichText::new(status).color(t.danger));
            }
            ui.add_space(4.0);
        });
        if let Some(role) = new_role {
            self.set_role(role);
        }
        if let Some(view) = new_view {
            self.set_view(view);
        }
        if reload {
            self.refresh();
        }

        match self.view {
            View::Chart => self.show_chart_view(ui),
            View::Research => {
                let rows = self.research.clone();
                egui::CentralPanel::default_margins().show(ui, |ui| match rows {
                    Some(rows) => research_view(ui, &rows),
                    None => {
                        ui.weak("The research data set needs the Researcher or Admin role.");
                    }
                });
            }
            View::AccessLog => {
                let entries = self.access_log.clone();
                egui::CentralPanel::default_margins().show(ui, |ui| match entries {
                    Some(entries) => access_log_view(ui, &entries, &t),
                    None => {
                        ui.weak("The access log needs the Admin role.");
                    }
                });
            }
        }
    }

    fn show_chart_view(&mut self, ui: &mut Ui) {
        let t = self.tokens;
        let mut clicked: Option<String> = None;
        egui::Panel::left("hc_patients").resizable(true).default_size(240.0).show(ui, |ui| {
            ui.add_space(6.0);
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
                    ui.weak(if self.status.is_some() { "Charts need the Clinician or Admin role." } else { "No patients yet." });
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

fn research_view(ui: &mut Ui, rows: &Value) {
    let rows = rows.as_array().cloned().unwrap_or_default();
    ui.heading("De-identified data set (HIPAA Safe Harbor)");
    ui.label("Names, addresses and record numbers removed; dates reduced to the year; ages over 89 grouped as 90+; ZIP codes cut to three digits (000 for sparsely populated areas); free text dropped. Research ids are random and unlinkable without the key the organization keeps.");
    ui.add_space(8.0);
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("hc_research").num_columns(7).striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
            for h in ["Research id", "Sex", "Born", "State", "ZIP3", "Conditions", "Latest results"] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            for r in &rows {
                ui.monospace(text(r, &["researchId"]).get(..8).unwrap_or_default());
                ui.label(text(r, &["sex"]));
                ui.label(if r.get("age90OrOver").and_then(Value::as_bool).unwrap_or(false) { "90+".to_owned() } else { text(r, &["birthYear"]) });
                ui.label(text(r, &["state"]));
                ui.label(text(r, &["zip3"]));
                let conditions: Vec<String> = r
                    .get("conditions")
                    .and_then(Value::as_array)
                    .map(|cs| cs.iter().map(|c| format!("{} ({})", coding_label(c.get("code").unwrap_or(&Value::Null)), text(c, &["onsetYear"]))).collect())
                    .unwrap_or_default();
                ui.label(conditions.join("\n"));
                let results: Vec<String> = r
                    .get("observations")
                    .and_then(Value::as_array)
                    .map(|os| {
                        os.iter()
                            .filter(|o| o.get("value").is_some_and(|v| !v.is_null()))
                            .map(|o| {
                                format!(
                                    "{}: {} ({})",
                                    short(&coding_label(o.get("code").unwrap_or(&Value::Null))),
                                    value_label(o.get("value").unwrap_or(&Value::Null)),
                                    text(o, &["year"])
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                ui.label(results.join("\n"));
                ui.end_row();
            }
        });
    });
}

fn access_log_view(ui: &mut Ui, entries: &Value, t: &Tokens) {
    let entries = entries.as_array().cloned().unwrap_or_default();
    ui.heading("Access log");
    ui.label(
        "Every command is recorded, including refusals. Each entry carries the hash of the one before it, so an edited or removed entry breaks the chain.",
    );
    ui.add_space(8.0);
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("hc_audit").num_columns(6).striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
            for h in ["#", "Time (UTC)", "Who", "Command", "Record", "Outcome"] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            for e in entries.iter().rev() {
                ui.label(text(e, &["sequence"]));
                ui.label(text(e, &["at"]).get(..19).unwrap_or_default().replace('T', " "));
                ui.label(format!("{} ({})", text(e, &["actor", "id"]), text(e, &["actor", "role"])));
                ui.monospace(text(e, &["command"]));
                let resource = text(e, &["resource"]);
                ui.monospace(resource.get(..16).map_or(resource.clone(), |s| format!("{s}…")));
                let outcome = text(e, &["outcome"]);
                let color = match outcome.as_str() {
                    "success" => t.accent,
                    "denied" => t.danger,
                    _ => t.warning,
                };
                ui.label(RichText::new(outcome).color(color));
                ui.end_row();
            }
        });
    });
}

/// A long LOINC display cut at its first bracket ("Cholesterol in LDL [Mass/volume]…" → "Cholesterol in LDL").
fn short(label: &str) -> String {
    label.split(['[', '/']).next().unwrap_or(label).trim().to_owned()
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
    if encounters.is_empty() {
        ui.weak("None recorded");
    }
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
    fn researcher_is_refused_identified_data_but_sees_the_research_set() {
        let mut app = ChartApp::new(loaded(), actor(Role::Researcher));
        assert!(app.patients().is_empty());
        assert!(app.status().unwrap().contains("may not run"));
        app.select(SYNTHETIC_PATIENT_ID);
        assert!(app.chart().is_none());
        frame(&mut app);
        app.set_view(View::Research);
        assert_eq!(app.status(), None);
        assert_eq!(app.research.as_ref().unwrap().as_array().unwrap().len(), 1);
        frame(&mut app);
        app.set_view(View::AccessLog);
        assert!(app.status().unwrap().contains("may not run"));
        frame(&mut app);
    }

    #[test]
    fn role_switcher_changes_what_is_visible() {
        let mut app = ChartApp::new(loaded(), actor(Role::Clinician)).with_role_switcher();
        assert_eq!(app.patients().len(), 1);
        app.set_role(Role::Admin);
        app.set_view(View::AccessLog);
        let log = app.access_log.as_ref().unwrap().as_array().unwrap();
        assert!(log.iter().any(|e| e["actor"]["id"] == "dr.test"), "the clinician's reads are in the log");
        frame(&mut app);
        assert_eq!(short("Cholesterol in LDL [Mass/volume] in Serum"), "Cholesterol in LDL");
        assert_eq!(short("Hemoglobin A1c/Hemoglobin.total in Blood"), "Hemoglobin A1c");
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
