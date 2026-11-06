pub mod users;

use axum::Router;

use crate::state::AppState;

/// API version 1. New versions get a sibling module, never breaking changes.
pub fn router() -> Router<AppState> {
    Router::new().nest("/users", users::router())
}
