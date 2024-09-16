use axum::{
    extract::{Path, State},
    response::Response,
    Form,
};
use garde::{Unvalidated, Validate};
use hyper::{Body, StatusCode};
use leptos::*;
use serde::Deserialize;

use crate::{
    auth::Auth,
    startup::AppState,
};

// TODO: separate schema for create and update when needed later on
#[derive(Deserialize, Validate, Debug)]
pub struct UpdateProjectOwnerRequest {
    #[garde(length(max = 128))]
    pub name: String,
}

#[tracing::instrument()]
pub async fn post(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Path(owner_id): Path<String>,
    Form(req): Form<Unvalidated<UpdateProjectOwnerRequest>>,
) -> Response<Body> {
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap()
}
