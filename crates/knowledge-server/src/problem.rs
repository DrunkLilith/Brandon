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

    #[must_use]
    pub fn unauthorized() -> Self {
        Self {
            problem_type: "https://knowledge.local/problems/authentication-required",
            title: "Authentication required",
            status: StatusCode::UNAUTHORIZED.as_u16(),
            detail: "The supplied credentials or session are invalid".to_owned(),
            code: "authentication_required",
        }
    }

    #[must_use]
    pub fn csrf() -> Self {
        Self {
            problem_type: "https://knowledge.local/problems/csrf-rejected",
            title: "CSRF validation failed",
            status: StatusCode::FORBIDDEN.as_u16(),
            detail: "A valid CSRF token is required".to_owned(),
            code: "csrf_rejected",
        }
    }

    #[must_use]
    pub fn cross_origin() -> Self {
        Self {
            problem_type: "https://knowledge.local/problems/cross-origin-rejected",
            title: "Cross-origin request rejected",
            status: StatusCode::FORBIDDEN.as_u16(),
            detail: "State-changing browser requests must be same-origin".to_owned(),
            code: "cross_origin_rejected",
        }
    }

    #[must_use]
    pub fn login_rate_limited() -> Self {
        Self {
            problem_type: "https://knowledge.local/problems/login-rate-limited",
            title: "Too many login attempts",
            status: StatusCode::TOO_MANY_REQUESTS.as_u16(),
            detail: "Try signing in again later".to_owned(),
            code: "login_rate_limited",
        }
    }

    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            problem_type: "https://knowledge.local/problems/service-unavailable",
            title: "Service unavailable",
            status: StatusCode::SERVICE_UNAVAILABLE.as_u16(),
            detail: "The service has not been configured".to_owned(),
            code: "service_unavailable",
        }
    }

    #[must_use]
    pub fn internal() -> Self {
        Self {
            problem_type: "https://knowledge.local/problems/internal-error",
            title: "Internal server error",
            status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            detail: "The request could not be completed".to_owned(),
            code: "internal_error",
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
