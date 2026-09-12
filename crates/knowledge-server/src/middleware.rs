use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};
use uuid::Uuid;

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
