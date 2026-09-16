use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use serde_json::{Value, json};

use crate::state::AppState;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/health/live", get(live))
        .route("/api/v1/health/ready", get(ready))
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let components = super::operational::readiness(&state).await;
    let ready = components.ready();
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({
            "status": if ready { "ready" } else { "not_ready" },
            "database": components.database,
            "migrations": components.migrations,
            "masterKey": components.master_key,
            "objectStorage": components.object_storage
        })),
    )
}

async fn live() -> Json<Value> {
    Json(json!({"status": "ok"}))
}
