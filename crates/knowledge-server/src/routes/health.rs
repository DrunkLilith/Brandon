use axum::{
    Json,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use serde_json::{Value, json};

use crate::state::AppState;

api_router! {
    pub(crate) fn router;
    "/api/v1/health/live" => [get => live => ("getLiveness", Public, 200)],
    "/api/v1/health/ready" => [get => ready => ("getReadiness", Public, 200)],
    "/api/v1/openapi.json" => [get => openapi => ("getOpenApi", Public, 200)],
}

async fn openapi() -> axum::response::Response {
    let mut response = Json(crate::openapi::document()).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    response
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
