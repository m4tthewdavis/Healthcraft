//! Workspace tooling: `cargo xtask <command>`.
//!
//! Pure Rust (std + serde_json). `cargo` is invoked through `std::process::Command`.

mod layers;

use std::path::PathBuf;
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  layers   enforce the crate dependency layering (docs/architecture.md)
  wasm     cargo check --target wasm32-unknown-unknown for the wasm-safe crates (L0-L4)
  ci       the CI gate: fmt --check, clippy -D warnings, layers, tests, wasm (stops at first failure)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("layers") => cmd_layers(),
        Some("wasm") => cmd_wasm(),
        Some("ci") => cmd_ci(),
        Some("-h" | "--help" | "help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().map(PathBuf::from).unwrap_or(manifest_dir)
}

fn cargo(args: &[&str]) -> Result<(), String> {
    eprintln!("$ cargo {}", args.join(" "));
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(args)
        .current_dir(root())
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("`cargo {}` failed", args.join(" "))) }
}

fn cmd_layers() -> Result<(), String> {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root())
        .output()
        .map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!("cargo metadata failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let crates = layers::from_metadata(&metadata)?;
    let violations = layers::check(&crates);
    if violations.is_empty() {
        eprintln!("layers: {} crates, no violations", crates.len());
        Ok(())
    } else {
        for v in &violations {
            eprintln!("  {v}");
        }
        Err(format!("{} layering violation(s)", violations.len()))
    }
}

fn cmd_wasm() -> Result<(), String> {
    let mut args = vec!["check", "--target", "wasm32-unknown-unknown"];
    for name in layers::wasm_crates() {
        args.push("-p");
        args.push(name);
    }
    cargo(&args)
}

fn cmd_ci() -> Result<(), String> {
    cargo(&["fmt", "--all", "--check"])?;
    cargo(&["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"])?;
    cmd_layers()?;
    cargo(&["test", "--workspace", "--locked"])?;
    cmd_wasm()
}
