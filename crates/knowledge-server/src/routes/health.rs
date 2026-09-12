use axum::{Json, Router, routing::get};
use serde_json::{Value, json};

use crate::state::AppState;

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/api/v1/health/live", get(live))
}

async fn live() -> Json<Value> {
    Json(json!({"status": "ok"}))
}
