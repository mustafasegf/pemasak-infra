use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::State, response::Response, Json
};
use hyper::{Body, StatusCode};
use secrecy::ExposeSecret;
use serde::Deserialize;
use crate::{startup::AppState, auth::{Auth, User, RegisterUserErrorType, ErrorResponse, Secret}};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: Secret<String>,
}

#[tracing::instrument(skip(auth, pool, password))]
pub async fn login_user(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Json(LoginRequest { username, password }): Json<LoginRequest>,
) -> Response<Body> {
    // get user
    let user = match User::get_from_username(&username, &pool).await {
        Ok(user) => user,
        Err(_err) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: "Wrong username or password entered".to_string(),
                error_type: RegisterUserErrorType::BadRequestError,
            }).unwrap();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
                .unwrap();
        }
    };

    // check password
    let hasher = Argon2::default();
    let hash = PasswordHash::new(&user.password).unwrap();
    if let Err(err) = hasher.verify_password(password.expose_secret().as_bytes(), &hash) {
        tracing::error!(?err, "Can't login: Failed to verify password");
        let json = serde_json::to_string(&ErrorResponse {
            message: "Wrong username or password entered".to_string(),
            error_type: RegisterUserErrorType::BadRequestError,
        }).unwrap();
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(json))
            .unwrap();
    };

    auth.login_user(user.id);
    Response::builder()
        .status(StatusCode::FOUND)
        .header("HX-Location", "/api/dashboard")
        .body(Body::empty())
        .unwrap()
}
