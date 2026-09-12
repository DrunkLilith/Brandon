//! HTTP server library for Knowledge Platform.

pub mod config;
pub mod middleware;
pub mod problem;
pub mod state;

mod routes;

use axum::{Router, middleware as axum_middleware};
use state::AppState;

/// Builds the complete HTTP router for the supplied application state.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(routes::health::router())
        .with_state(state)
        .layer(axum_middleware::from_fn(middleware::request_id))
}

/// Builds a router backed by dependency-free state for contract tests.
pub fn build_test_router() -> Router {
    build_router(AppState)
}
