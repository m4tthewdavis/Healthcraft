//! The clinician UI, drawn with egui.
//!
//! The UI is a thin client of the engine: everything it shows comes from a command
//! (`patient.list`, `patient.chart`, `research.deidentify`, `audit.entries`), and everything it
//! changes goes through one (`encounter.open`, `encounter.close`, `note.sign`). It never reads the
//! store, so what a user can see is exactly what their role allows and every view is in the access
//! log.
//!
//! Layout: a top bar (who you are, which workspace), and for the chart a patient list beside a
//! patient banner with four tabs: Summary, Encounters (each visit with its results and notes),
//! Notes, and Results (a flowsheet).

use std::sync::Arc;

use egui::{RichText, Ui};
use healthcraft_core::{Actor, Role};
use healthcraft_engine::Session;
use serde_json::{Value, json};

mod chart;
pub mod model;
pub mod theme;
mod views;

use model::{ChartModel, text};
use theme::Tokens;

/// One row of the patient list.
#[derive(Debug, Clone, PartialEq)]
pub struct PatientRow {
    pub id: String,
    pub name: String,
    pub birth_date: String,
}

/// The three workspaces. Each one is backed by engine commands, so switching to one the role may
/// not use shows the engine's refusal (and the refusal is in the access log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Identified charts, for care.
    Chart,
    /// The Safe Harbor data set.
    Research,
    /// Who did what.
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

/// The tabs of an open chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartTab {
    Summary,
    Encounters,
    Notes,
    Results,
}

impl ChartTab {
    pub const ALL: [ChartTab; 4] = [ChartTab::Summary, ChartTab::Encounters, ChartTab::Notes, ChartTab::Results];

    pub fn label(self) -> &'static str {
        match self {
            ChartTab::Summary => "Summary",
            ChartTab::Encounters => "Encounters",
            ChartTab::Notes => "Notes",
            ChartTab::Results => "Results",
        }
    }
}

/// A note being written; nothing is stored until it is signed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NoteDraft {
    pub title: String,
    pub text: String,
    pub encounter: Option<String>,
}

/// What a click asked for. Collected while drawing, applied after the frame's layout.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    ChartTab(ChartTab),
    SelectEncounter(String),
    SelectNote(String),
    StartVisit,
    EndVisit(String),
    NewNote(Option<String>),
    DiscardNote,
    SignNote,
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
    model: Option<ChartModel>,
    tab: ChartTab,
    selected_encounter: Option<String>,
    selected_note: Option<String>,
    draft: Option<NoteDraft>,
    research: Option<Value>,
    access_log: Option<Value>,
    status: Option<String>,
    notice: Option<String>,
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
            model: None,
            tab: ChartTab::Summary,
            selected_encounter: None,
            selected_note: None,
            draft: None,
            research: None,
            access_log: None,
            status: None,
            notice: None,
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

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn chart(&self) -> Option<&Value> {
        self.chart.as_ref()
    }

    pub fn model(&self) -> Option<&ChartModel> {
        self.model.as_ref()
    }

    pub fn view(&self) -> View {
        self.view
    }

    pub fn tab(&self) -> ChartTab {
        self.tab
    }

    pub fn draft_mut(&mut self) -> Option<&mut NoteDraft> {
        self.draft.as_mut()
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
        self.draft = None;
        self.notice = None;
        self.refresh();
    }

    pub fn set_view(&mut self, view: View) {
        self.view = view;
        self.notice = None;
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
                self.model = None;
                return;
            }
        }
        if let Some(id) = self.selected.clone() {
            self.reload_chart(&id);
        }
    }

    fn reload_chart(&mut self, patient_id: &str) {
        self.chart = self.load("patient.chart", &json!({ "patientId": patient_id }));
        self.model = self.chart.as_ref().map(ChartModel::from_json);
    }

    pub fn select(&mut self, patient_id: &str) {
        if self.selected.as_deref() != Some(patient_id) {
            self.selected_encounter = None;
            self.selected_note = None;
            self.draft = None;
            self.notice = None;
        }
        self.selected = Some(patient_id.to_owned());
        self.status = None;
        self.reload_chart(patient_id);
    }

    /// Runs a write command for the open chart, then reloads it. Returns the reply on success.
    fn write(&mut self, command: &str, params: Value, done: &str) -> Option<Value> {
        let reply = self.load(command, &params)?;
        self.notice = Some(done.to_owned());
        if let Some(id) = self.selected.clone() {
            self.reload_chart(&id);
        }
        Some(reply)
    }

    /// Applies one user action (also what tests and agents use to drive the UI).
    pub fn apply(&mut self, action: Action) {
        let Some(patient) = self.selected.clone() else { return };
        self.status = None;
        match action {
            Action::ChartTab(tab) => self.tab = tab,
            Action::SelectEncounter(id) => self.selected_encounter = Some(id),
            Action::SelectNote(id) => {
                self.selected_note = Some(id);
                self.draft = None;
            }
            Action::StartVisit => {
                let params = json!({ "patientId": patient, "class": "ambulatory", "reason": "Office visit" });
                if let Some(reply) = self.write("encounter.open", params, "Visit started") {
                    self.selected_encounter = reply.get("encounterId").and_then(Value::as_str).map(str::to_owned);
                    self.tab = ChartTab::Encounters;
                }
            }
            Action::EndVisit(id) => {
                self.write("encounter.close", json!({ "patientId": patient, "encounterId": id }), "Visit ended");
            }
            Action::NewNote(encounter) => {
                self.draft = Some(NoteDraft { title: "Progress note".into(), text: String::new(), encounter });
                self.tab = ChartTab::Notes;
            }
            Action::DiscardNote => self.draft = None,
            Action::SignNote => {
                let Some(draft) = self.draft.clone() else { return };
                let params = match &draft.encounter {
                    Some(e) => json!({ "patientId": patient, "encounterId": e, "title": draft.title.trim(), "text": draft.text.trim() }),
                    None => json!({ "patientId": patient, "title": draft.title.trim(), "text": draft.text.trim() }),
                };
                if let Some(reply) = self.write("note.sign", params, "Note signed") {
                    self.draft = None;
                    self.selected_note = reply.get("noteId").and_then(Value::as_str).map(str::to_owned);
                }
            }
        }
    }

    /// Draws the whole window into `ui` (the root `Ui` of a frame).
    pub fn show(&mut self, ui: &mut Ui) {
        let t = self.tokens;
        let mut new_role = None;
        let mut new_view = None;
        let mut reload = false;
        egui::Panel::top("hc_top").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("HealthCraft").size(theme::TITLE - 4.0).color(t.accent).strong());
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
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Synthetic data only. Not certified health IT; not for clinical use.").size(theme::SMALL).color(t.warning));
                if let Some(notice) = &self.notice {
                    ui.label(RichText::new(notice).size(theme::SMALL).color(t.ok));
                }
            });
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
                    Some(rows) => views::research_view(ui, &rows),
                    None => {
                        ui.weak("The research data set needs the Researcher or Admin role.");
                    }
                });
            }
            View::AccessLog => {
                let entries = self.access_log.clone();
                egui::CentralPanel::default_margins().show(ui, |ui| match entries {
                    Some(entries) => views::access_log_view(ui, &entries, &t),
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
        egui::Panel::left("hc_patients").resizable(true).default_size(230.0).show(ui, |ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("Patients").size(theme::SECTION).strong());
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Search name"));
            ui.add_space(4.0);
            let needle = self.filter.to_lowercase();
            egui::ScrollArea::vertical().id_salt("hc_patient_list").auto_shrink([false, false]).show(ui, |ui| {
                for p in self.patients.iter().filter(|p| needle.is_empty() || p.name.to_lowercase().contains(&needle)) {
                    let selected = self.selected.as_deref() == Some(p.id.as_str());
                    if chart::list_item(ui, selected, &format!("{}\n{}", p.name, p.birth_date)).clicked() {
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

        let today = self.session.now().date_naive();
        let mut actions = Vec::new();
        let (model, tab) = (self.model.as_ref(), self.tab);
        let (selected_encounter, selected_note) = (self.selected_encounter.clone(), self.selected_note.clone());
        let draft = &mut self.draft;
        egui::CentralPanel::default_margins().show(ui, |ui| match model {
            Some(model) => chart::ChartUi {
                model,
                tab,
                selected_encounter: selected_encounter.as_deref(),
                selected_note: selected_note.as_deref(),
                draft,
                today,
                tokens: t,
                actions: &mut actions,
            }
            .show(ui),
            None => {
                ui.weak("Select a patient.");
            }
        });
        for action in actions {
            self.apply(action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use healthcraft_testkit::{SYNTHETIC_BUNDLE, SYNTHETIC_PATIENT_ID, actor, session};

    fn loaded() -> Arc<Session> {
        let s = session();
        s.dispatch(&actor(Role::System), "import.fhirBundle", &json!({ "bundle": SYNTHETIC_BUNDLE })).unwrap();
        healthcraft_engine::demo::seed(&s).unwrap();
        Arc::new(s)
    }

    fn frame(app: &mut ChartApp) {
        let ctx = egui::Context::default();
        for size in [egui::vec2(1280.0, 800.0), egui::vec2(420.0, 800.0)] {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)), ..Default::default() };
            for _ in 0..2 {
                ctx.run_ui(input.clone(), |ui| app.show(ui)).drop_without_applying_deltas();
            }
        }
    }

    fn draw_every_tab(app: &mut ChartApp) {
        for tab in ChartTab::ALL {
            app.apply(Action::ChartTab(tab));
            frame(app);
        }
    }

    #[test]
    fn clinician_sees_the_list_and_every_chart_tab() {
        let mut app = ChartApp::new(loaded(), actor(Role::Clinician));
        assert_eq!(app.patients().len(), 7);
        app.select(SYNTHETIC_PATIENT_ID);
        let model = app.model().unwrap();
        assert_eq!(model.name, "Synthetic Testpatient");
        assert_eq!(model.encounters.len(), 2);
        draw_every_tab(&mut app);
        assert_eq!(app.status(), None);
    }

    #[test]
    fn clinician_starts_a_visit_signs_a_note_and_ends_the_visit() {
        let mut app = ChartApp::new(loaded(), actor(Role::Clinician));
        app.select(SYNTHETIC_PATIENT_ID);
        app.apply(Action::StartVisit);
        assert_eq!(app.notice(), Some("Visit started"));
        let open = app.model().unwrap().encounters.iter().find(|e| e.end.is_none()).unwrap().id.clone();
        app.apply(Action::NewNote(Some(open.clone())));
        assert_eq!(app.tab(), ChartTab::Notes);
        frame(&mut app);
        app.draft_mut().unwrap().text = "S: Here for follow-up.\nP: Return in 3 months.".into();
        app.apply(Action::SignNote);
        assert_eq!(app.notice(), Some("Note signed"), "{:?}", app.status());
        let model = app.model().unwrap();
        assert_eq!(model.notes[0].encounter.as_deref(), Some(open.as_str()));
        assert_eq!(model.notes[0].author, "dr.test");
        app.apply(Action::EndVisit(open));
        assert!(app.model().unwrap().encounters.iter().all(|e| e.end.is_some()));
        draw_every_tab(&mut app);
    }

    #[test]
    fn an_empty_note_is_refused_by_the_engine() {
        let mut app = ChartApp::new(loaded(), actor(Role::Clinician));
        app.select(SYNTHETIC_PATIENT_ID);
        app.apply(Action::NewNote(None));
        app.apply(Action::SignNote);
        assert!(app.status().unwrap().contains("text"), "{:?}", app.status());
        assert!(app.draft_mut().is_some(), "the draft is kept so nothing typed is lost");
    }

    #[test]
    fn researcher_is_refused_identified_data_but_sees_the_research_set() {
        let mut app = ChartApp::new(loaded(), actor(Role::Researcher));
        assert!(app.patients().is_empty());
        assert!(app.status().unwrap().contains("may not run"));
        app.select(SYNTHETIC_PATIENT_ID);
        assert!(app.model().is_none());
        frame(&mut app);
        app.set_view(View::Research);
        assert_eq!(app.status(), None);
        assert_eq!(app.research.as_ref().unwrap().as_array().unwrap().len(), 7);
        frame(&mut app);
        app.set_view(View::AccessLog);
        assert!(app.status().unwrap().contains("may not run"));
        frame(&mut app);
    }

    #[test]
    fn role_switcher_changes_what_is_visible() {
        let mut app = ChartApp::new(loaded(), actor(Role::Clinician)).with_role_switcher();
        assert_eq!(app.patients().len(), 7);
        app.set_role(Role::Admin);
        app.set_view(View::AccessLog);
        let log = app.access_log.as_ref().unwrap().as_array().unwrap();
        assert!(log.iter().any(|e| e["actor"]["id"] == "dr.test"), "the clinician's reads are in the log");
        frame(&mut app);
    }

    #[test]
    fn theme_tokens_are_opaque() {
        let t = Tokens::default();
        for c in [t.accent, t.warning, t.danger, t.muted, t.ok] {
            assert_eq!(c.a(), 255);
        }
    }
}
