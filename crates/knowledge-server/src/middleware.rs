use axum::{
    extract::{Request, State},
    http::{HeaderValue, Method, header},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

use crate::{
    problem::ApiProblem,
    routes::auth::verify_request_csrf,
    state::{AppState, canonical_origin},
};

pub const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestId(Uuid);

impl RequestId {
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

pub async fn request_id(mut request: Request, next: Next) -> Response {
    let request_id = RequestId(Uuid::now_v7());
    let header_value = HeaderValue::from_str(&request_id.0.to_string())
        .expect("a UUID always forms a valid HTTP header value");

    request.headers_mut().remove(REQUEST_ID_HEADER);
    request.extensions_mut().insert(request_id);

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(REQUEST_ID_HEADER, header_value);
    response
}

pub async fn request_security(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiProblem> {
    if is_unsafe_method(request.method()) {
        validate_same_origin(&state, &request)?;
        if request.uri().path() != "/api/v1/auth/login" {
            verify_request_csrf(request.headers())?;
        }
    }
    Ok(next.run(request).await)
}

fn is_unsafe_method(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn validate_same_origin(state: &AppState, request: &Request) -> Result<(), ApiProblem> {
    if request
        .headers()
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("cross-site"))
    {
        return Err(ApiProblem::cross_origin());
    }

    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(ApiProblem::cross_origin)?;
    let origin = canonical_origin(origin).map_err(|_| ApiProblem::cross_origin())?;
    let expected = state
        .runtime()
        .map(|runtime| runtime.public_origin.as_str())
        .ok_or_else(ApiProblem::unavailable)?;
    if origin != expected {
        return Err(ApiProblem::cross_origin());
    }
    Ok(())
}
