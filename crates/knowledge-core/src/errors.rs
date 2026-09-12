use thiserror::Error;

/// Failures produced by pure domain operations.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DomainError {
    #[error("resource was not found")]
    NotFound,
    #[error("operation is not permitted")]
    Forbidden,
    #[error("resource version conflicts with the current version")]
    Conflict,
    #[error("invalid value for {field}: {message}")]
    Validation {
        field: &'static str,
        message: String,
    },
}
