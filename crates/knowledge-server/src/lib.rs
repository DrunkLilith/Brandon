//! HTTP server library for Knowledge Platform.

mod audit;
pub mod bootstrap;
pub mod config;
pub mod middleware;
pub mod problem;
pub mod state;

mod routes;

use axum::{Router, middleware as axum_middleware};
pub use routes::auth::CurrentUser;
use state::AppState;

/// Builds the complete HTTP router for the supplied application state.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(routes::health::router())
        .merge(routes::auth::router())
        .with_state(state.clone())
        .layer(axum_middleware::from_fn_with_state(
            state,
            middleware::request_security,
        ))
        .layer(axum_middleware::from_fn(middleware::request_id))
}

/// Builds a router backed by dependency-free state for contract tests.
pub fn build_test_router() -> Router {
    build_router(AppState::default())
}
