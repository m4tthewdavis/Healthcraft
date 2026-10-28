//! Research projections.

use chrono::NaiveDate;
use healthcraft_core::Actor;
use healthcraft_research::{deid, omop};
use serde::Deserialize;
use serde_json::Value;

use crate::{Access, CommandSpec, EngineError, Session, params, to_json};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec { id: "research.omop.export", label: "Export OMOP CDM", params: "{}", access: Access::Export, run: omop_export },
        CommandSpec {
            id: "research.deidentify",
            label: "De-identified data set (Safe Harbor)",
            params: r#"{"asOf"?:"YYYY-MM-DD"}"#,
            access: Access::Research,
            run: deidentify,
        },
    ]
}

fn charts(session: &Session) -> Result<Vec<healthcraft_record::Chart>, EngineError> {
    session.store.patients().into_iter().map(|p| session.chart(p)).collect()
}

fn omop_export(session: &Session, _: &Actor, _: &Value) -> Result<Value, EngineError> {
    to_json(&omop::export(&charts(session)?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeidParams {
    #[serde(default)]
    as_of: Option<NaiveDate>,
}

fn deidentify(session: &Session, _: &Actor, value: &Value) -> Result<Value, EngineError> {
    let p: DeidParams = params(value)?;
    let as_of = p.as_of.unwrap_or_else(|| session.clock.now().date_naive());
    let charts = charts(session)?;
    let mut key = session.pseudonyms();
    let rows: Vec<_> = charts.iter().map(|c| deid::deidentify(c, as_of, &mut key, session.ids.as_ref())).collect();
    to_json(&rows)
}
