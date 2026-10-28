//! The HTTP API.
//!
//! | Route | Command |
//! |---|---|
//! | `GET /healthz` | none (no auth; liveness only) |
//! | `GET /api/commands` | `commands.list` |
//! | `POST /api/commands/{id}` (JSON params) | any command |
//! | `GET /fhir/r4/Patient` | `fhir.patient.search` |
//! | `GET /fhir/r4/Patient/{id}` | `fhir.patient.read` |
//! | `GET /fhir/r4/Patient/{id}/$everything` | `fhir.patient.everything` |
//!
//! Every route but `/healthz` needs `Authorization: Bearer <token>`; each token maps to one
//! [`Actor`], and the engine decides what that actor may do. Tokens are compared in constant
//! time. This is development-grade authentication: SMART on FHIR / OAuth 2.0 is on the roadmap,
//! and the server binds to localhost by default.

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use healthcraft_core::Actor;
use healthcraft_engine::{EngineError, Session};
use healthcraft_store::StoreError;
use serde_json::{Value, json};

/// Shortest bearer token accepted (256 bits as hex is 64 characters).
pub const MIN_TOKEN_LEN: usize = 32;
/// Largest request body (FHIR bundle imports are the big ones).
pub const MAX_BODY_BYTES: usize = 140 << 20;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("at least one access token is required")]
    NoTokens,
    #[error("access token for `{0}` is shorter than {MIN_TOKEN_LEN} characters")]
    WeakToken(String),
    #[error("two actors share one access token")]
    DuplicateToken,
}

#[derive(Clone)]
pub struct ApiConfig {
    tokens: Arc<Vec<(String, Actor)>>,
}

impl ApiConfig {
    pub fn new(tokens: Vec<(String, Actor)>) -> Result<Self, ConfigError> {
        if tokens.is_empty() {
            return Err(ConfigError::NoTokens);
        }
        for (i, (token, actor)) in tokens.iter().enumerate() {
            if token.len() < MIN_TOKEN_LEN {
                return Err(ConfigError::WeakToken(actor.id.clone()));
            }
            if tokens.iter().skip(i.saturating_add(1)).any(|(other, _)| other == token) {
                return Err(ConfigError::DuplicateToken);
            }
        }
        Ok(Self { tokens: Arc::new(tokens) })
    }

    fn actor_for(&self, headers: &HeaderMap) -> Option<Actor> {
        let presented = headers.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ")?.trim();
        // Check every token so the time taken does not reveal which one (or how much) matched.
        let mut found = None;
        for (token, actor) in self.tokens.iter() {
            if constant_time_eq(token.as_bytes(), presented.as_bytes()) {
                found = Some(actor.clone());
            }
        }
        found
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Clone)]
struct AppState {
    session: Arc<Session>,
    config: ApiConfig,
}

pub fn router(session: Arc<Session>, config: ApiConfig) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/commands", get(list_commands))
        .route("/api/commands/{id}", post(run_command))
        .route("/fhir/r4/Patient", get(fhir_search))
        .route("/fhir/r4/Patient/{id}", get(fhir_read))
        .route("/fhir/r4/Patient/{id}/$everything", get(fhir_everything))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(AppState { session, config })
}

/// Runs a command on a blocking thread (the engine is synchronous) and maps the result to HTTP.
async fn dispatch(state: &AppState, headers: &HeaderMap, command: String, params: Value, fhir: bool) -> Response {
    let Some(actor) = state.config.actor_for(headers) else {
        let mut response = error_response(StatusCode::UNAUTHORIZED, "missing or unknown bearer token", fhir);
        response.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return response;
    };
    let session = state.session.clone();
    let joined = tokio::task::spawn_blocking(move || session.dispatch(&actor, &command, &params)).await;
    match joined {
        Ok(Ok(value)) => {
            let mut response = Json(value).into_response();
            if fhir {
                response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/fhir+json"));
            }
            response
        }
        Ok(Err(e)) => error_response(status_of(&e), &e.to_string(), fhir),
        Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "command did not complete", fhir),
    }
}

fn status_of(e: &EngineError) -> StatusCode {
    match e {
        EngineError::UnknownCommand(_) | EngineError::Store(StoreError::NotFound(_)) => StatusCode::NOT_FOUND,
        EngineError::Denied { .. } => StatusCode::FORBIDDEN,
        EngineError::BadParams(_) | EngineError::Store(StoreError::BadBatch) => StatusCode::BAD_REQUEST,
        EngineError::Store(StoreError::Conflict { .. }) => StatusCode::CONFLICT,
        EngineError::Store(StoreError::Rejected(_)) => StatusCode::UNPROCESSABLE_ENTITY,
        EngineError::Failed(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// A JSON error body; for FHIR routes, an `OperationOutcome` as the FHIR spec requires.
fn error_response(status: StatusCode, message: &str, fhir: bool) -> Response {
    if fhir {
        let body = json!({
            "resourceType": "OperationOutcome",
            "issue": [{ "severity": "error", "code": if status == StatusCode::NOT_FOUND { "not-found" } else { "processing" }, "diagnostics": message }]
        });
        let mut response = (status, Json(body)).into_response();
        response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/fhir+json"));
        response
    } else {
        (status, Json(json!({ "error": message }))).into_response()
    }
}

async fn list_commands(State(state): State<AppState>, headers: HeaderMap) -> Response {
    dispatch(&state, &headers, "commands.list".into(), Value::Null, false).await
}

async fn run_command(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>, body: Option<Json<Value>>) -> Response {
    dispatch(&state, &headers, id, body.map(|Json(v)| v).unwrap_or(Value::Null), false).await
}

async fn fhir_search(State(state): State<AppState>, headers: HeaderMap) -> Response {
    dispatch(&state, &headers, "fhir.patient.search".into(), Value::Null, true).await
}

async fn fhir_read(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    dispatch(&state, &headers, "fhir.patient.read".into(), json!({ "patientId": id }), true).await
}

async fn fhir_everything(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    dispatch(&state, &headers, "fhir.patient.everything".into(), json!({ "patientId": id }), true).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use healthcraft_core::Role;
    use healthcraft_testkit::{SYNTHETIC_BUNDLE, SYNTHETIC_PATIENT_ID, actor, session};
    use tower::ServiceExt;

    const CLINICIAN: &str = "clinician-token-0123456789abcdef0123456789";
    const RESEARCHER: &str = "researcher-token-0123456789abcdef012345678";
    const SYSTEM: &str = "system-token-0123456789abcdef0123456789abcd";

    fn app() -> Router {
        let config = ApiConfig::new(vec![
            (CLINICIAN.into(), actor(Role::Clinician)),
            (RESEARCHER.into(), actor(Role::Researcher)),
            (SYSTEM.into(), actor(Role::System)),
        ])
        .unwrap();
        router(Arc::new(session()), config)
    }

    async fn call(app: &Router, method: &str, uri: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, HeaderMap, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())).unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, headers, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    #[tokio::test]
    async fn requires_a_valid_token() {
        let app = app();
        let (status, headers, _) = call(&app, "GET", "/fhir/r4/Patient", None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(headers[header::WWW_AUTHENTICATE], "Bearer");
        let (status, _, body) = call(&app, "GET", "/fhir/r4/Patient", Some("wrong-token-0123456789abcdef0123456789ab"), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["resourceType"], "OperationOutcome");
        let (status, _, _) = call(&app, "GET", "/healthz", None, None).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn imports_then_serves_fhir() {
        let app = app();
        let (status, _, body) = call(&app, "POST", "/api/commands/import.fhirBundle", Some(SYSTEM), Some(json!({ "bundle": SYNTHETIC_BUNDLE }))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (status, headers, body) = call(&app, "GET", &format!("/fhir/r4/Patient/{SYNTHETIC_PATIENT_ID}"), Some(CLINICIAN), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "application/fhir+json");
        assert_eq!(body["resourceType"], "Patient");
        assert_eq!(body["birthDate"], "1961-04-12");
        let (_, _, bundle) = call(&app, "GET", &format!("/fhir/r4/Patient/{SYNTHETIC_PATIENT_ID}/$everything"), Some(CLINICIAN), None).await;
        assert_eq!(bundle["type"], "collection");
        let (_, _, search) = call(&app, "GET", "/fhir/r4/Patient", Some(CLINICIAN), None).await;
        assert_eq!(search["total"], 1);
    }

    #[tokio::test]
    async fn maps_engine_errors_to_status_codes() {
        let app = app();
        let (status, _, _) = call(&app, "GET", &format!("/fhir/r4/Patient/{SYNTHETIC_PATIENT_ID}"), Some(RESEARCHER), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _, body) = call(&app, "GET", "/fhir/r4/Patient/00000000-0000-0000-0000-000000000009", Some(CLINICIAN), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["issue"][0]["code"], "not-found");
        let (status, _, _) = call(&app, "GET", "/fhir/r4/Patient/not-a-uuid", Some(CLINICIAN), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _, _) = call(&app, "POST", "/api/commands/no.such", Some(CLINICIAN), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _, body) = call(&app, "POST", "/api/commands/patient.register", Some(CLINICIAN), Some(json!({"family": 1}))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("invalid parameters"));
        let (status, _, list) = call(&app, "GET", "/api/commands", Some(RESEARCHER), None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(list.as_array().unwrap().len() > 10);
    }

    #[test]
    fn config_rejects_weak_missing_and_duplicate_tokens() {
        assert_eq!(ApiConfig::new(vec![]).err(), Some(ConfigError::NoTokens));
        assert_eq!(ApiConfig::new(vec![("short".into(), actor(Role::Admin))]).err(), Some(ConfigError::WeakToken("admin.test".into())));
        let dup = vec![(CLINICIAN.into(), actor(Role::Clinician)), (CLINICIAN.into(), actor(Role::Admin))];
        assert_eq!(ApiConfig::new(dup).err(), Some(ConfigError::DuplicateToken));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
