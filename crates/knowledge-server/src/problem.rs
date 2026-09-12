use axum::{
    Json,
    http::{HeaderValue, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApiProblem {
    #[serde(rename = "type")]
    problem_type: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
    code: &'static str,
}

impl ApiProblem {
    #[must_use]
    pub fn validation(field: &str, message: &str) -> Self {
        Self {
            problem_type: "https://knowledge.local/problems/validation-failed",
            title: "Validation failed",
            status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
            detail: format!("{field}: {message}"),
            code: "validation_failed",
        }
    }
}

impl IntoResponse for ApiProblem {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, Json(self)).into_response();
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}
