use axum::response::Response;
use hyper::{Body, StatusCode};
use crate::auth::Auth;

#[tracing::instrument(skip(auth))]
pub async fn logout_user(auth: Auth) -> Response<Body> {
    auth.logout_user();
    Response::builder()
        .status(StatusCode::FOUND)
        .header("Location", "/api/login")
        .body(Body::empty())
        .unwrap()
}