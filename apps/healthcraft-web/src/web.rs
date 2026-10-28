//! The browser shell: the clock and id source the domain needs (wasm32 has neither without
//! JavaScript), the demo data, and eframe's web runner.

use std::sync::Arc;

use chrono::DateTime;
use healthcraft_core::{Actor, Clock, IdSource, Role, Timestamp, Uuid};
use healthcraft_engine::Session;
use healthcraft_store::InMemoryStore;
use healthcraft_ui_egui::ChartApp;
use serde_json::json;
use wasm_bindgen::JsCast;

const CANVAS_ID: &str = "healthcraft_canvas";
const LOADING_ID: &str = "healthcraft_loading";
const SYNTHETIC_BUNDLE: &str = include_str!("../../../fixtures/synthetic-bundle.json");

/// The browser's clock (`Date.now()`).
struct BrowserClock;

impl Clock for BrowserClock {
    fn now(&self) -> Timestamp {
        let ms = js_sys::Date::now();
        // Finite and well inside i64 for any real date; fall back to the epoch otherwise.
        let ms = if ms.is_finite() { ms as i64 } else { 0 };
        DateTime::from_timestamp_millis(ms).unwrap_or_default()
    }
}

/// Version 4 UUIDs from `crypto.getRandomValues`, falling back to `Math.random` where the Web
/// Crypto API is unavailable (the demo holds synthetic data only).
struct BrowserIds;

impl IdSource for BrowserIds {
    fn next_uuid(&self) -> Uuid {
        let mut bytes = [0u8; 16];
        let filled = web_sys::window().and_then(|w| w.crypto().ok()).is_some_and(|c| c.get_random_values_with_u8_array(&mut bytes).is_ok());
        if !filled {
            for b in &mut bytes {
                *b = (js_sys::Math::random() * 256.0) as u8;
            }
        }
        uuid::Builder::from_random_bytes(bytes).into_uuid()
    }
}

fn demo_session() -> Session {
    let session = Session::new(Arc::new(InMemoryStore::default()), Arc::new(BrowserClock), Arc::new(BrowserIds));
    let system = Actor::system();
    if let Err(e) = session.dispatch(&system, "import.fhirBundle", &json!({ "bundle": SYNTHETIC_BUNDLE })) {
        log::error!("healthcraft-web: fixture import failed: {e}");
    }
    if let Err(e) = healthcraft_engine::demo::seed(&session) {
        log::error!("healthcraft-web: demo cohort failed: {e}");
    }
    session
}

pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            log::error!("healthcraft-web: no document");
            return;
        };
        let Some(canvas) = document.get_element_by_id(CANVAS_ID).and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok()) else {
            log::error!("healthcraft-web: missing <canvas id=\"{CANVAS_ID}\">");
            return;
        };
        let session = Arc::new(demo_session());
        let mut app = ChartApp::new(session, Actor { id: "dr.demo".into(), role: Role::Clinician }).with_role_switcher();
        if let Some(first) = app.patients().first().map(|p| p.id.clone()) {
            app.select(&first);
        }
        let result = eframe::WebRunner::new().start(canvas, eframe::WebOptions::default(), Box::new(move |_cc| Ok(Box::new(WebShell(app))))).await;
        if let Some(el) = document.get_element_by_id(LOADING_ID) {
            match result {
                Ok(()) => el.remove(),
                Err(e) => el.set_inner_html(&format!("<p>HealthCraft could not start: {e:?}</p><p>It needs a browser with WebGL2.</p>")),
            }
        }
    });
}

struct WebShell(ChartApp);

impl eframe::App for WebShell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.show(ui);
    }
}
