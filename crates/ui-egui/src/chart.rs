//! The chart: a patient banner over four tabs (Summary, Encounters, Notes, Results). Drawing
//! functions only read the model and push [`Action`]s; the app applies them afterwards through
//! engine commands.

use chrono::NaiveDate;
use egui::{Align, Layout, RichText, ScrollArea, TextEdit, Ui};

use crate::model::{ChartModel, Encounter, clock, day};
use crate::theme::{self, BODY, SECTION, SMALL, TITLE, Tokens};
use crate::{Action, ChartTab, NoteDraft};

const LIST_WIDTH: f32 = 300.0;

pub struct ChartUi<'a> {
    pub model: &'a ChartModel,
    pub tab: ChartTab,
    pub selected_encounter: Option<&'a str>,
    pub selected_note: Option<&'a str>,
    pub draft: &'a mut Option<NoteDraft>,
    pub today: NaiveDate,
    pub tokens: Tokens,
    pub actions: &'a mut Vec<Action>,
}

impl ChartUi<'_> {
    pub fn show(&mut self, ui: &mut Ui) {
        self.banner(ui);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            for tab in ChartTab::ALL {
                let label = RichText::new(tab.label()).size(SECTION);
                if ui.selectable_label(self.tab == tab, label).clicked() && self.tab != tab {
                    self.actions.push(Action::ChartTab(tab));
                }
            }
        });
        ui.separator();
        ui.add_space(4.0);
        match self.tab {
            ChartTab::Summary => ScrollArea::vertical().id_salt("hc_summary").auto_shrink([false, false]).show(ui, |ui| self.summary(ui)).inner,
            ChartTab::Encounters => self.encounters(ui),
            ChartTab::Notes => self.notes(ui),
            ChartTab::Results => ScrollArea::both().id_salt("hc_results").auto_shrink([false, false]).show(ui, |ui| self.results(ui)).inner,
        }
    }

    fn banner(&self, ui: &mut Ui) {
        let m = self.model;
        let t = self.tokens;
        theme::banner(ui, &t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(&m.name).size(TITLE).strong());
            let age = m.age_on(self.today).map(|a| format!("{a} y")).unwrap_or_default();
            let mut facts = vec![capitalize(&m.sex), age, format!("DOB {}", m.birth_date)];
            if !m.city_state.is_empty() {
                facts.push(m.city_state.clone());
            }
            facts.push(format!("ID {}", m.patient_id.get(..8).unwrap_or(&m.patient_id)));
            ui.label(RichText::new(facts.into_iter().filter(|f| !f.is_empty()).collect::<Vec<_>>().join("  ·  ")).size(BODY).color(t.muted));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                theme::chip(ui, &plural(m.conditions.len(), "problem"), t.accent);
                if let Some(last) = m.encounters.iter().find(|e| e.end.is_some()) {
                    theme::chip(ui, &format!("Last visit {}", day(&last.start)), t.accent);
                }
                if let Some(open) = m.encounters.iter().find(|e| e.end.is_none()) {
                    theme::chip(ui, &format!("In visit: {}", open.title()), t.ok);
                }
                theme::chip(ui, "Synthetic patient", t.warning);
            });
        });
    }

    fn summary(&mut self, ui: &mut Ui) {
        let m = self.model;
        let t = self.tokens;
        let wide = ui.available_width() > 760.0;
        let mut left = |ui: &mut Ui| {
            section(ui, "Problem list", |ui| {
                if m.conditions.is_empty() {
                    ui.weak("None recorded");
                }
                for c in &m.conditions {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&c.label).size(BODY));
                        if !c.onset.is_empty() {
                            ui.label(RichText::new(format!("since {}", day(&c.onset))).size(SMALL).color(t.muted));
                        }
                    });
                }
            });
            ui.add_space(10.0);
            section(ui, "Recent visits", |ui| {
                if m.encounters.is_empty() {
                    ui.weak("None recorded");
                }
                for e in m.encounters.iter().take(4) {
                    let row = ui.horizontal(|ui| {
                        ui.label(RichText::new(day(&e.start)).size(BODY).monospace());
                        ui.label(RichText::new(e.title()).size(BODY));
                        ui.label(RichText::new(e.class_label()).size(SMALL).color(t.muted));
                    });
                    if row.response.interact(egui::Sense::click()).on_hover_text("Open this visit").clicked() {
                        self.actions.push(Action::SelectEncounter(e.id.clone()));
                        self.actions.push(Action::ChartTab(ChartTab::Encounters));
                    }
                }
            });
        };
        let right = |ui: &mut Ui| {
            section(ui, "Latest results", |ui| {
                let sheet = m.flowsheet();
                if sheet.is_empty() {
                    ui.weak("No results");
                }
                egui::Grid::new("hc_latest").num_columns(4).spacing([14.0, 6.0]).show(ui, |ui| {
                    for s in &sheet {
                        let Some((date, value, _)) = s.values.first() else { continue };
                        ui.label(RichText::new(&s.test).size(BODY));
                        ui.label(RichText::new(value).size(BODY).strong());
                        trend(ui, s.trend(), &t);
                        ui.label(RichText::new(date).size(SMALL).color(t.muted));
                        ui.end_row();
                    }
                });
            });
            ui.add_space(10.0);
            section(ui, "Latest note", |ui| match m.notes.first() {
                Some(n) => {
                    ui.label(RichText::new(&n.title).size(BODY).strong());
                    ui.label(RichText::new(format!("Signed by {} · {} {}", n.author, day(&n.signed), clock(&n.signed))).size(SMALL).color(t.muted));
                    ui.add_space(4.0);
                    ui.label(RichText::new(&n.text).size(BODY));
                }
                None => {
                    ui.weak("No notes");
                }
            });
        };
        if wide {
            ui.columns(2, |cols| {
                if let [l, r] = cols {
                    left(l);
                    right(r);
                }
            });
        } else {
            left(ui);
            ui.add_space(10.0);
            right(ui);
        }
    }

    fn encounters(&mut self, ui: &mut Ui) {
        let m = self.model;
        let t = self.tokens;
        let selected = self.selected_encounter.and_then(|id| m.encounter(id)).or_else(|| m.encounters.first());
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(LIST_WIDTH, ui.available_height()), Layout::top_down(Align::Min), |ui| {
                ui.set_width(LIST_WIDTH);
                if ui.button("Start visit").on_hover_text("Open a new office visit for this patient, starting now").clicked() {
                    self.actions.push(Action::StartVisit);
                }
                ui.add_space(6.0);
                ScrollArea::vertical().id_salt("hc_enc_list").auto_shrink([false, false]).show(ui, |ui| {
                    if m.encounters.is_empty() {
                        ui.weak("No visits yet");
                    }
                    for e in &m.encounters {
                        let is_selected = selected.is_some_and(|s| s.id == e.id);
                        let label = format!("{}   {}\n{}", day(&e.start), e.class_label(), e.title());
                        if list_item(ui, is_selected, &label).clicked() {
                            self.actions.push(Action::SelectEncounter(e.id.clone()));
                        }
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| match selected {
                Some(e) => ScrollArea::vertical().id_salt("hc_enc_detail").auto_shrink([false, false]).show(ui, |ui| self.encounter_detail(ui, e, &t)).inner,
                None => {
                    ui.weak("Select a visit.");
                }
            });
        });
    }

    fn encounter_detail(&mut self, ui: &mut Ui, e: &Encounter, t: &Tokens) {
        let m = self.model;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(e.title()).size(TITLE - 4.0).strong());
            match &e.end {
                None => theme::chip(ui, "Open", t.ok),
                Some(_) => theme::chip(ui, "Closed", t.muted),
            }
        });
        let when = match &e.end {
            Some(end) => format!("{} · {} {}–{} UTC", e.class_label(), day(&e.start), clock(&e.start), clock(end)),
            None => format!("{} · started {} {} UTC", e.class_label(), day(&e.start), clock(&e.start)),
        };
        ui.label(RichText::new(when).size(BODY).color(t.muted));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Write note").clicked() {
                self.actions.push(Action::NewNote(Some(e.id.clone())));
            }
            if e.end.is_none() && ui.button("End visit").clicked() {
                self.actions.push(Action::EndVisit(e.id.clone()));
            }
        });
        ui.add_space(10.0);
        section(ui, "Results from this visit", |ui| {
            let results: Vec<_> = m.observations.iter().filter(|o| o.encounter.as_deref() == Some(e.id.as_str())).collect();
            if results.is_empty() {
                ui.weak("None");
            }
            egui::Grid::new("hc_visit_results").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                for o in results {
                    let style = |s: &str| if o.entered_in_error { RichText::new(s).strikethrough().color(t.muted) } else { RichText::new(s) };
                    ui.label(style(&o.test).size(BODY));
                    ui.label(style(&o.value).size(BODY).strong());
                    ui.end_row();
                }
            });
        });
        ui.add_space(10.0);
        let notes: Vec<_> = m.notes.iter().filter(|n| n.encounter.as_deref() == Some(e.id.as_str())).collect();
        section(ui, "Notes", |ui| {
            if notes.is_empty() {
                ui.weak("No notes for this visit yet");
            }
            for n in notes {
                note_body(ui, n, t);
                ui.add_space(6.0);
            }
        });
    }

    fn notes(&mut self, ui: &mut Ui) {
        let m = self.model;
        let t = self.tokens;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(LIST_WIDTH, ui.available_height()), Layout::top_down(Align::Min), |ui| {
                ui.set_width(LIST_WIDTH);
                if ui.button("New note").clicked() {
                    let open = m.encounters.iter().find(|e| e.end.is_none()).map(|e| e.id.clone());
                    self.actions.push(Action::NewNote(open));
                }
                ui.add_space(6.0);
                ScrollArea::vertical().id_salt("hc_note_list").auto_shrink([false, false]).show(ui, |ui| {
                    if m.notes.is_empty() {
                        ui.weak("No notes yet");
                    }
                    for n in &m.notes {
                        let is_selected = self.draft.is_none()
                            && (self.selected_note == Some(n.id.as_str()) || (self.selected_note.is_none() && m.notes.first().is_some_and(|f| f.id == n.id)));
                        let visit = n.encounter.as_deref().and_then(|id| m.encounter(id)).map(Encounter::title).unwrap_or_else(|| "No visit".into());
                        let label = format!("{}   {}\n{} · {}", day(&n.signed), n.title, visit, n.author);
                        if list_item(ui, is_selected, &label).clicked() {
                            self.actions.push(Action::SelectNote(n.id.clone()));
                        }
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                if self.draft.is_some() {
                    self.editor(ui);
                    return;
                }
                let note = self.selected_note.and_then(|id| m.notes.iter().find(|n| n.id == id)).or_else(|| m.notes.first());
                match note {
                    Some(n) => {
                        ScrollArea::vertical().id_salt("hc_note_read").auto_shrink([false, false]).show(ui, |ui| note_body(ui, n, &t));
                    }
                    None => {
                        ui.weak("Select a note, or write a new one.");
                    }
                }
            });
        });
    }

    fn editor(&mut self, ui: &mut Ui) {
        let m = self.model;
        let t = self.tokens;
        let Some(draft) = self.draft.as_mut() else { return };
        ui.label(RichText::new("New note").size(TITLE - 4.0).strong());
        ui.add_space(6.0);
        egui::Grid::new("hc_note_form").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Title");
            ui.add(TextEdit::singleline(&mut draft.title).desired_width(320.0));
            ui.end_row();
            ui.label("Visit");
            let current = draft
                .encounter
                .as_deref()
                .and_then(|id| m.encounter(id))
                .map(|e| format!("{}  {}", day(&e.start), e.title()))
                .unwrap_or_else(|| "No visit".into());
            egui::ComboBox::from_id_salt("hc_note_visit").width(320.0).selected_text(current).show_ui(ui, |ui| {
                ui.selectable_value(&mut draft.encounter, None, "No visit");
                for e in &m.encounters {
                    ui.selectable_value(&mut draft.encounter, Some(e.id.clone()), format!("{}  {}", day(&e.start), e.title()));
                }
            });
            ui.end_row();
        });
        ui.add_space(6.0);
        ui.add(TextEdit::multiline(&mut draft.text).hint_text("S:\nO:\nA:\nP:").desired_rows(14).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let ready = !draft.title.trim().is_empty() && !draft.text.trim().is_empty();
            if ui
                .add_enabled(ready, egui::Button::new(RichText::new("Sign note").strong()))
                .on_hover_text("Signing is final: corrections are written as an addendum")
                .clicked()
            {
                self.actions.push(Action::SignNote);
            }
            if ui.button("Discard").clicked() {
                self.actions.push(Action::DiscardNote);
            }
            ui.label(RichText::new("Signed notes cannot be edited.").size(SMALL).color(t.muted));
        });
    }

    fn results(&mut self, ui: &mut Ui) {
        let m = self.model;
        let t = self.tokens;
        let sheet = m.flowsheet();
        if sheet.is_empty() {
            ui.weak("No results");
            return;
        }
        let dates: Vec<String> = m.result_dates().into_iter().take(8).collect();
        ui.label(RichText::new("Newest on the left. Arrows compare each test's latest value with the one before it.").size(SMALL).color(t.muted));
        ui.add_space(6.0);
        egui::Grid::new("hc_flowsheet").striped(true).spacing([18.0, 8.0]).show(ui, |ui| {
            ui.label(RichText::new("Test").strong());
            ui.label(RichText::new("Trend").strong());
            for d in &dates {
                ui.label(RichText::new(d).strong().monospace());
            }
            ui.end_row();
            for s in &sheet {
                ui.label(RichText::new(&s.test).size(BODY));
                trend(ui, s.trend(), &t);
                for d in &dates {
                    let cell = s.values.iter().find(|(date, _, _)| date == d).map(|(_, v, _)| v.as_str()).unwrap_or("");
                    ui.label(RichText::new(cell).size(BODY));
                }
                ui.end_row();
            }
        });
    }
}

/// A full-width, left-aligned selectable row.
pub fn list_item(ui: &mut Ui, selected: bool, label: &str) -> egui::Response {
    ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| ui.selectable_label(selected, RichText::new(label).size(BODY))).inner
}

fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).size(SECTION).strong());
        ui.add_space(6.0);
        add(ui);
    });
}

fn note_body(ui: &mut Ui, n: &crate::model::Note, t: &Tokens) {
    theme::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(&n.title).size(SECTION).strong());
        ui.label(RichText::new(format!("Signed by {} · {} {} UTC", n.author, day(&n.signed), clock(&n.signed))).size(SMALL).color(t.muted));
        ui.add_space(6.0);
        ui.label(RichText::new(&n.text).size(BODY));
    });
}

fn trend(ui: &mut Ui, trend: Option<i8>, t: &Tokens) {
    let (symbol, hint) = match trend {
        Some(1) => ("↑", "Higher than the previous result"),
        Some(-1) => ("↓", "Lower than the previous result"),
        Some(_) => ("=", "Same as the previous result"),
        None => ("", ""),
    };
    let response = ui.label(RichText::new(symbol).size(BODY).color(t.muted));
    if !hint.is_empty() {
        response.on_hover_text(hint);
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
