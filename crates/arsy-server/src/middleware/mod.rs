use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};

/// Header carrying the per-request correlation id.
pub const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Assigns a UUID to every request, exposes it to downstream handlers through
/// the request extensions, and echoes it back on the response.
pub async fn request_id(mut req: Request, next: Next) -> Response {
    let id = uuid::Uuid::new_v4();
    req.extensions_mut().insert(id);

    let mut res = next.run(req).await;
    if let Ok(value) = HeaderValue::try_from(id.to_string()) {
        res.headers_mut().insert(REQUEST_ID.clone(), value);
    }
    res
}
