pub mod v1;

use axum::{Router, http::StatusCode, middleware, routing::get};
use tower_http::trace::TraceLayer;

use crate::{errors::ApiResult, middleware::request_id, state::AppState};

/// Top-level router: health check plus versioned API, with shared layers.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .nest("/v1", v1::router())
        .layer(middleware::from_fn(request_id))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn healthz() -> ApiResult<StatusCode> {
    Ok(StatusCode::OK)
}
