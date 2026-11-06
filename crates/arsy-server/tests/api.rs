//! End-to-end checks against the exact router the binary serves.

use std::sync::Arc;

use arsy_core::services::UserService;
use arsy_infra::db::InMemoryUserRepository;
use arsy_server::{routes::router, state::AppState};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

fn test_app() -> Router {
    let state = AppState::new(Arc::new(UserService::new(Arc::new(
        InMemoryUserRepository::default(),
    ))));
    router(state)
}

async fn read_json(res: axum::response::Response) -> Value {
    let bytes = res
        .into_body()
        .collect()
        .await
        .expect("failed to read body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("response is not valid JSON")
}

#[tokio::test]
async fn healthz_returns_ok_with_request_id() {
    let res = test_app()
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers().contains_key("x-request-id"));
}

#[tokio::test]
async fn create_then_list_and_get_user() {
    let app = test_app();

    let res = app
        .clone()
        .oneshot(
            Request::post("/v1/users")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "name": " Ada ", "email": "ADA@example.com" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let created = read_json(res).await;
    assert_eq!(created["name"], "Ada");
    assert_eq!(created["email"], "ada@example.com");
    let id = created["id"].as_str().unwrap().to_string();

    let res = app
        .clone()
        .oneshot(Request::get("/v1/users").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let listed = read_json(res).await;
    assert_eq!(listed.as_array().unwrap().len(), 1);

    let res = app
        .oneshot(
            Request::get(format!("/v1/users/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn invalid_payload_maps_to_400_envelope() {
    let res = test_app()
        .oneshot(
            Request::post("/v1/users")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "name": "", "email": "bad" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = read_json(res).await;
    assert_eq!(body["error"]["status"], 400);
}

#[tokio::test]
async fn missing_user_maps_to_404() {
    let id = uuid::Uuid::new_v4();
    let res = test_app()
        .oneshot(
            Request::get(format!("/v1/users/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = read_json(res).await;
    assert_eq!(body["error"]["status"], 404);
}
