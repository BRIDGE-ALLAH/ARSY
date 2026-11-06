use arsy_core::error::CoreError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// HTTP-visible error: a status code plus a client-safe message.
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl From<CoreError> for ApiError {
    fn from(err: CoreError) -> Self {
        let status = match &err {
            CoreError::NotFound(_) => StatusCode::NOT_FOUND,
            CoreError::InvalidInput(_) => StatusCode::BAD_REQUEST,
            CoreError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            // Keep internals out of the response body, but leave them in the logs.
            tracing::error!(error = ?err, "internal error while handling request");
            Self::new(status, "internal server error")
        } else {
            Self::new(status, err.to_string())
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": {
                "status": self.status.as_u16(),
                "message": self.message,
            }
        });
        (self.status, Json(body)).into_response()
    }
}

/// Handler return type: anything that converts into a response or an [`ApiError`].
pub type ApiResult<T> = Result<T, ApiError>;
