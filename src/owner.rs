use axum::{Router, extract::{State, Path}, response::Response, Form, routing::{post, put}};
use garde::{Unvalidated, Validate};
use hyper::{Body, StatusCode};
use serde::Deserialize;
use uuid::Uuid;

use crate::{configuration::Settings, startup::AppState, auth::Auth};

// TODO: separate schema for create and update when needed later on
#[derive(Deserialize, Validate, Debug)]
pub struct OwnerRequest {
    #[garde(length(max=128))]
    pub name: String,
}

#[tracing::instrument(skip(auth, pool))]
pub async fn remove_project_member(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Path((project_id, user_id)): Path<(Uuid, Uuid)>
) -> Response<Body> {
    Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap()
}

#[tracing::instrument(skip(auth, pool))]
pub async fn invite_project_member(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Path((project_id, user_id)): Path<(Uuid, Uuid)>
) -> Response<Body> {
    Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap()
}

#[tracing::instrument(skip(auth, pool))]
pub async fn create_project_owner(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Form(req):  Form<Unvalidated<OwnerRequest>>,
) -> Response<Body> {
    Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap()
}

#[tracing::instrument()]
pub async fn update_project_owner(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Path(project_id): Path<String>,
    Form(req): Form<Unvalidated<OwnerRequest>>
) -> Response<Body> {
    Response::builder().status(StatusCode::NO_CONTENT).body(Body::empty()).unwrap()
}

pub fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {    
    Router::new()
        .route("/owner", post(create_project_owner))
        .route("/owner/:project_id", post(update_project_owner))
        .route("/owner/:project_id/:user_id", post(invite_project_member).delete(remove_project_member))
}
