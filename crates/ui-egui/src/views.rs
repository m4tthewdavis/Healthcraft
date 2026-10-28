//! The research and access-log views (no patient selected; one command each).

use egui::{RichText, Ui};
use serde_json::Value;

use crate::model::{coding_label, short, text, value_label};
use crate::theme::Tokens;

pub fn research_view(ui: &mut Ui, rows: &Value) {
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

pub fn access_log_view(ui: &mut Ui, entries: &Value, t: &Tokens) {
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
