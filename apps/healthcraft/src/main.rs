//! HealthCraft desktop: the clinician UI in a native window, over an in-memory session.
//!
//! Starts with the synthetic fixture patient loaded (or the FHIR bundles given with `--import`).
//! Nothing is saved: this is a demo shell until the durable store lands (docs/roadmap.md).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, ValueEnum};
use healthcraft_core::{Actor, Role};
use healthcraft_engine::Session;
use healthcraft_ui_egui::ChartApp;
use serde_json::json;

const SYNTHETIC_BUNDLE: &str = include_str!("../../../fixtures/synthetic-bundle.json");
const MAX_FILE_BYTES: u64 = 140 << 20;

#[derive(Clone, Copy, ValueEnum)]
enum RoleArg {
    Clinician,
    Researcher,
    Admin,
}

#[derive(Parser)]
#[command(name = "healthcraft", version, about = "HealthCraft desktop (synthetic data only)")]
struct Args {
    /// FHIR bundles to load instead of the built-in synthetic patient (e.g. Synthea output).
    #[arg(long, value_name = "BUNDLE.json")]
    import: Vec<PathBuf>,
    /// Who you are in this session.
    #[arg(long, default_value = "dr.demo")]
    user: String,
    #[arg(long, value_enum, default_value = "clinician")]
    role: RoleArg,
    /// Open this patient's chart at startup (default: the first patient in the list).
    #[arg(long, value_name = "UUID")]
    patient: Option<String>,
}

struct DesktopApp(ChartApp);

impl eframe::App for DesktopApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.show(ui);
    }
}

fn load(session: &Session, args: &Args) -> Result<(), String> {
    let system = Actor::system();
    let bundles: Vec<String> = if args.import.is_empty() {
        vec![SYNTHETIC_BUNDLE.to_owned()]
    } else {
        args.import
            .iter()
            .map(|path| {
                let len = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
                if len > MAX_FILE_BYTES {
                    return Err(format!("{}: larger than {MAX_FILE_BYTES} bytes", path.display()));
                }
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
            })
            .collect::<Result<_, _>>()?
    };
    for bundle in bundles {
        session.dispatch(&system, "import.fhirBundle", &json!({ "bundle": bundle })).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = Args::parse();
    let role = match args.role {
        RoleArg::Clinician => Role::Clinician,
        RoleArg::Researcher => Role::Researcher,
        RoleArg::Admin => Role::Admin,
    };
    let actor = match Actor::new(args.user.clone(), role) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("healthcraft: {e}");
            return ExitCode::FAILURE;
        }
    };
    let session = Arc::new(Session::in_memory());
    if let Err(e) = load(&session, &args) {
        eprintln!("healthcraft: {e}");
        return ExitCode::FAILURE;
    }
    let options =
        eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_title("HealthCraft").with_inner_size([1280.0, 800.0]), ..Default::default() };
    let mut app = ChartApp::new(session, actor);
    if let Some(id) = args.patient.clone().or_else(|| app.patients().first().map(|p| p.id.clone())) {
        app.select(&id);
    }
    // Last-resort guard: the windowing stack panics when a system library is missing (for example
    // libxkbcommon-x11 on a minimal Linux). Report that instead of crashing (AGENTS.md › Never crash).
    let run =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| eframe::run_native("HealthCraft", options, Box::new(|_cc| Ok(Box::new(DesktopApp(app)))))));
    match run {
        Ok(Ok(())) => ExitCode::SUCCESS,
        Ok(Err(e)) => {
            eprintln!("healthcraft: {e}");
            ExitCode::FAILURE
        }
        Err(_) => {
            eprintln!("healthcraft: could not open a window (see the message above); on Linux, install libxkbcommon-x11 and a GPU or Mesa driver");
            ExitCode::FAILURE
        }
    }
}
