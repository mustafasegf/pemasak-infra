use axum::{
    extract::State, response::Response
};
use hyper::{Body, StatusCode};
use serde::Serialize;
use uuid::Uuid;
use crate::{startup::AppState, auth::{Auth, User, RegisterUserErrorType, ErrorResponse}};

#[derive(Serialize, Debug)]
pub struct ValidateAuthResponse {
    id: Uuid,
    username: String,
    name: String,
}

#[tracing::instrument(skip(auth))]
pub async fn validate_auth(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
) -> Response<Body> {
    if auth.current_user.is_none() {
        return Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::empty())
            .unwrap()
    }

    let current_user = auth.current_user.unwrap();
    let user = match User::get_from_username(&current_user.username, &pool).await {
        Ok(user) => user,
        Err(_err) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: "User not found".to_string(),
                error_type: RegisterUserErrorType::BadRequestError,
            }).unwrap();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
                .unwrap();
        }
    };

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(
            serde_json::to_string(
                &ValidateAuthResponse {
                    id: user.id,
                    username: user.username,
                    name: user.name,
                }
            ).unwrap()
        ))
        .unwrap()
}