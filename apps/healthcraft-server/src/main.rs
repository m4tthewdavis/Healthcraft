//! HealthCraft server: the HTTP API over an in-memory session.
//!
//! Access tokens come from the environment, one per role you want to enable (at least 32
//! characters each; generate with `openssl rand -hex 32`):
//!
//! ```text
//! HEALTHCRAFT_TOKEN_CLINICIAN, HEALTHCRAFT_TOKEN_RESEARCHER, HEALTHCRAFT_TOKEN_ADMIN, HEALTHCRAFT_TOKEN_SYSTEM
//! ```
//!
//! There is no TLS yet, so the server refuses to listen on anything but loopback unless
//! `--allow-remote` is given (put a TLS-terminating proxy in front if you do).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use healthcraft_api::{ApiConfig, router};
use healthcraft_core::{Actor, Role};
use healthcraft_engine::Session;
use serde_json::json;

const MAX_FILE_BYTES: u64 = 140 << 20;

#[derive(Parser)]
#[command(name = "healthcraft-server", version, about = "HealthCraft HTTP API (synthetic data only)")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:8080")]
    bind: SocketAddr,
    /// Listen on a non-loopback address (no TLS: only behind a TLS proxy).
    #[arg(long)]
    allow_remote: bool,
    /// FHIR bundles to load at startup (e.g. Synthea output).
    #[arg(long, value_name = "BUNDLE.json")]
    import: Vec<PathBuf>,
}

fn tokens() -> Vec<(String, Actor)> {
    [
        ("HEALTHCRAFT_TOKEN_CLINICIAN", "clinician", Role::Clinician),
        ("HEALTHCRAFT_TOKEN_RESEARCHER", "researcher", Role::Researcher),
        ("HEALTHCRAFT_TOKEN_ADMIN", "admin", Role::Admin),
        ("HEALTHCRAFT_TOKEN_SYSTEM", "system", Role::System),
    ]
    .into_iter()
    .filter_map(|(var, id, role)| std::env::var(var).ok().map(|token| (token, Actor { id: id.into(), role })))
    .collect()
}

fn import(session: &Session, paths: &[PathBuf]) -> Result<(), String> {
    let system = Actor::system();
    for path in paths {
        let len = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
        if len > MAX_FILE_BYTES {
            return Err(format!("{}: larger than {MAX_FILE_BYTES} bytes", path.display()));
        }
        let bundle = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let report = session.dispatch(&system, "import.fhirBundle", &json!({ "bundle": bundle })).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!(
            "imported {}: {} patient(s), {} event(s)",
            path.display(),
            report.get("patients").and_then(|p| p.as_array()).map_or(0, Vec::len),
            report.get("events").cloned().unwrap_or_default()
        );
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    if !args.bind.ip().is_loopback() && !args.allow_remote {
        eprintln!("healthcraft-server: refusing to listen on {} without TLS; pass --allow-remote behind a TLS proxy", args.bind);
        return ExitCode::FAILURE;
    }
    let config = match ApiConfig::new(tokens()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("healthcraft-server: {e} (set HEALTHCRAFT_TOKEN_CLINICIAN etc.; see --help)");
            return ExitCode::FAILURE;
        }
    };
    let session = Arc::new(Session::in_memory());
    if let Err(e) = import(&session, &args.import) {
        eprintln!("healthcraft-server: {e}");
        return ExitCode::FAILURE;
    }
    let listener = match tokio::net::TcpListener::bind(args.bind).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("healthcraft-server: cannot listen on {}: {e}", args.bind);
            return ExitCode::FAILURE;
        }
    };
    eprintln!("healthcraft-server: listening on http://{}", args.bind);
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    match axum::serve(listener, router(session, config)).with_graceful_shutdown(shutdown).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("healthcraft-server: {e}");
            ExitCode::FAILURE
        }
    }
}
