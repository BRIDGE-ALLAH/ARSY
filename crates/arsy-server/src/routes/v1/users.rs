use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use uuid::Uuid;

use arsy_core::models::{NewUser, User};

use crate::{errors::ApiResult, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(one))
}

async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<User>>> {
    Ok(Json(state.users.list()?))
}

async fn one(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Json<User>> {
    Ok(Json(state.users.get(id)?))
}

async fn create(
    State(state): State<AppState>,
    Json(input): Json<NewUser>,
) -> ApiResult<(StatusCode, Json<User>)> {
    let user = state.users.create(input)?;
    Ok((StatusCode::CREATED, Json(user)))
}
