//! HealthCraft command line. Each run builds an in-memory session from the bundles it is given,
//! runs one task and prints JSON (or writes files).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use healthcraft_core::{Actor, Role};
use healthcraft_engine::Session;
use healthcraft_research::omop;
use serde_json::{Value, json};

const MAX_FILE_BYTES: u64 = 140 << 20;

#[derive(Parser)]
#[command(name = "healthcraft-cli", version, about = "HealthCraft command line (synthetic data only)")]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Import FHIR bundles and print what was imported and skipped.
    Import { bundles: Vec<PathBuf> },
    /// Import FHIR bundles and print them back as a FHIR Patient searchset.
    Fhir { bundles: Vec<PathBuf> },
    /// Import FHIR bundles and write OMOP CDM tables as CSV into a directory.
    Omop {
        bundles: Vec<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Import FHIR bundles and print a Safe Harbor de-identified data set.
    Deid {
        bundles: Vec<PathBuf>,
        /// Date ages are computed on (default: today).
        #[arg(long, value_name = "YYYY-MM-DD")]
        as_of: Option<String>,
    },
    /// Parse an HL7 v2 message and print it (and its lab results, for ORU) as JSON.
    Hl7 { message: PathBuf },
    /// List every engine command with its parameters.
    Commands,
}

fn read(path: &Path) -> Result<String, String> {
    let len = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    if len > MAX_FILE_BYTES {
        return Err(format!("{}: larger than {MAX_FILE_BYTES} bytes", path.display()));
    }
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn session_with(bundles: &[PathBuf]) -> Result<(Session, Value), String> {
    let session = Session::in_memory();
    let system = Actor::system();
    let mut reports = Vec::new();
    for path in bundles {
        let report = session.dispatch(&system, "import.fhirBundle", &json!({ "bundle": read(path)? })).map_err(|e| format!("{}: {e}", path.display()))?;
        reports.push(json!({ "file": path.display().to_string(), "report": report }));
    }
    Ok((session, Value::Array(reports)))
}

fn as_role(role: Role) -> Actor {
    Actor { id: "healthcraft-cli".into(), role }
}

fn run(cli: Cli) -> Result<Value, String> {
    match cli.task {
        Task::Import { bundles } => Ok(session_with(&bundles)?.1),
        Task::Fhir { bundles } => {
            let (s, _) = session_with(&bundles)?;
            s.dispatch(&as_role(Role::Admin), "fhir.patient.search", &Value::Null).map_err(|e| e.to_string())
        }
        Task::Omop { bundles, out } => {
            let (s, _) = session_with(&bundles)?;
            let tables = s.dispatch(&as_role(Role::Admin), "research.omop.export", &Value::Null).map_err(|e| e.to_string())?;
            let tables: omop::Tables = serde_json::from_value(tables).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            let mut written = Vec::new();
            for (name, csv) in tables.to_csv_files() {
                let path = out.join(name);
                std::fs::write(&path, csv).map_err(|e| format!("{}: {e}", path.display()))?;
                written.push(path.display().to_string());
            }
            Ok(json!({ "written": written, "notExported": tables.not_exported }))
        }
        Task::Deid { bundles, as_of } => {
            let (s, _) = session_with(&bundles)?;
            let params = match as_of {
                Some(d) => json!({ "asOf": d }),
                None => Value::Null,
            };
            s.dispatch(&as_role(Role::Researcher), "research.deidentify", &params).map_err(|e| e.to_string())
        }
        Task::Hl7 { message } => {
            let msg = healthcraft_hl7v2::Message::parse(&read(&message)?).map_err(|e| e.to_string())?;
            let results = match msg.message_type() {
                Some(("ORU", _)) => serde_json::to_value(healthcraft_hl7v2::oru::lab_results(&msg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?,
                _ => Value::Null,
            };
            Ok(json!({ "message": msg, "labResults": results }))
        }
        Task::Commands => Session::in_memory().dispatch(&as_role(Role::System), "commands.list", &Value::Null).map_err(|e| e.to_string()),
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(value) => match serde_json::to_string_pretty(&value) {
            Ok(text) => {
                println!("{text}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("healthcraft-cli: {e}");
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("healthcraft-cli: {e}");
            ExitCode::FAILURE
        }
    }
}
