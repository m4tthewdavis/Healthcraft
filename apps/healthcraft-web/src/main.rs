//! HealthCraft in the browser.
//!
//! Runs the same [`healthcraft_ui_egui::ChartApp`] as the desktop app through eframe's web runner
//! (WebGL2), over an in-memory session holding the synthetic fixture patient and the demo cohort.
//! Nothing leaves the browser tab and nothing is saved. Build with `cargo xtask web`.

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("healthcraft-web only runs in the browser: build it with `cargo xtask web`");
}
