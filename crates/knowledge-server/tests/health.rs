use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1024)
        .await
        .expect("health response body should be readable");
    serde_json::from_slice(&bytes).expect("health response body should be JSON")
}

#[tokio::test]
async fn live_health_returns_ok() {
    let response = knowledge_server::build_test_router()
        .oneshot(
            Request::builder()
                .uri("/api/v1/health/live")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should answer");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await, json!({"status": "ok"}));
}
