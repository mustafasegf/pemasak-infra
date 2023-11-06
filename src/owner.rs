use axum::{Router, extract::{State, Path}, response::Response, Form, routing::{post, put}};
use garde::{Unvalidated, Validate};
use hyper::{Body, StatusCode};
use leptos::{ssr::render_to_string, svg::view};
use serde::Deserialize;
use ulid::Ulid;
use uuid::Uuid;
use leptos::*;

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

#[tracing::instrument(skip(_auth, pool))]
pub async fn create_project_owner(
    _auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Form(req):  Form<Unvalidated<OwnerRequest>>,
) -> Response<Body> {
    let data = match req.validate(&()) {
        Ok(valid) => valid.into_inner(),
        Err(err) => {
            let html = render_to_string(move || { view! {
                <p> {err.to_string() } </p>
            }}).into_owned();
            return Response::builder().status(StatusCode::BAD_REQUEST).body(Body::from(html)).unwrap();
        }
    };

    // Check for existing project
    match sqlx::query!(
        r#"SELECT id FROM project_owners
        WHERE name = $1
        "#,
        data.name
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(None) => (),
        Ok(Some(_)) => {
            tracing::error!("Project owner already exists with the following name: {}", data.name);

            let html = render_to_string(move || { view! {
                <p> Project with name {data.name} already exists </p>
            }}).into_owned();

            return Response::builder().status(StatusCode::BAD_REQUEST).body(Body::from(html)).unwrap();
        },
        Err(err) => {
            tracing::error!(?err, "Can't get existing project owner: Failed to query database");

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap()
        }
    };

    let project_id = Uuid::from(Ulid::new());

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!(?err, "Can't insert project owner: Failed to begin transaction");
            let html = render_to_string(move || { view! {
                <h1> Failed to begin transaction {err.to_string()} </h1>
            }}).into_owned();

            return Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
        }
    };

    if let Err(err) = sqlx::query!(
        r#"INSERT INTO project_owners (id, name)
        VALUES ($1, $2)
        "#,
        project_id,
        data.name
    )
    .execute(&mut *tx)
    .await {
        tracing::error!(?err, "Can't insert project owner: Failed to insert into database");
        if let Err(err) = tx.rollback().await {
            tracing::error!(?err, "Can't insert project owner: Failed to rollback transaction");
        }

        let html = render_to_string(move || { view! {
            <h1> Failed to insert project owner into database </h1>
        }}).into_owned();

        return Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
    }

    Response::builder().status(StatusCode::OK)
        .header("Content-Type", "text/html")
        .body(Body::empty())
        .unwrap()
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
