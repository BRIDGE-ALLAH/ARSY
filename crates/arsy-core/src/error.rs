use thiserror::Error;

/// Domain-level error shared by services and storage ports.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("resource not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("internal error: {0}")]
    Internal(String),
}
