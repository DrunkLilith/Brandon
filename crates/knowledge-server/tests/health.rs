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

#[tokio::test]
async fn live_health_includes_a_server_request_id() {
    let response = knowledge_server::build_test_router()
        .oneshot(
            Request::builder()
                .uri("/api/v1/health/live")
                .header("x-request-id", "untrusted-client-value")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should answer");

    let request_id = response
        .headers()
        .get("x-request-id")
        .expect("request ID response header should exist")
        .to_str()
        .expect("request ID should be ASCII");
    assert_ne!(request_id, "untrusted-client-value");
    assert!(uuid::Uuid::parse_str(request_id).is_ok());
}
