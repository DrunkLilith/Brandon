use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header::CONTENT_TYPE},
    middleware,
    routing::post,
};
use knowledge_server::{middleware::request_id, problem::ApiProblem};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn invalid_request() -> Result<(), ApiProblem> {
    Err(ApiProblem::validation(
        "email",
        "must be a valid email address",
    ))
}

#[tokio::test]
async fn validation_error_is_problem_json_with_server_request_id() {
    let app = Router::new()
        .route("/invalid", post(invalid_request))
        .layer(middleware::from_fn(request_id));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/invalid")
                .header("x-request-id", "client-controlled")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should answer");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response.headers()[CONTENT_TYPE], "application/problem+json");
    let request_id = response
        .headers()
        .get("x-request-id")
        .expect("request ID response header should exist")
        .to_str()
        .expect("request ID should be ASCII");
    assert_ne!(request_id, "client-controlled");
    assert!(uuid::Uuid::parse_str(request_id).is_ok());

    let bytes = to_bytes(response.into_body(), 4096)
        .await
        .expect("problem response should be readable");
    let body: Value = serde_json::from_slice(&bytes).expect("problem response should be JSON");
    assert_eq!(
        body,
        json!({
            "type": "https://knowledge.local/problems/validation-failed",
            "title": "Validation failed",
            "status": 422,
            "detail": "email: must be a valid email address",
            "code": "validation_failed"
        })
    );
}
