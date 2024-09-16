use axum::{
    extract::State,
    response::Response,
    Form,
};
use garde::{Unvalidated, Validate};
use hyper::{Body, StatusCode};
use leptos::ssr::render_to_string;
use leptos::*;
use serde::Deserialize;
use ulid::Ulid;
use uuid::Uuid;

use crate::{
    auth::Auth,
    startup::AppState,
};

// TODO: separate schema for create and update when needed later on
#[derive(Deserialize, Validate, Debug)]
pub struct CreateProjectOwnerRequest {
    #[garde(length(max = 128))]
    pub name: String,
}

#[tracing::instrument(skip(_auth, pool))]
pub async fn post(
    _auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Form(req): Form<Unvalidated<CreateProjectOwnerRequest>>,
) -> Response<Body> {
    let data = match req.validate(&()) {
        Ok(valid) => valid.into_inner(),
        Err(err) => {
            let html = render_to_string(move || {
                view! {
                    <p> {err.to_string() } </p>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(html))
                .unwrap();
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
            tracing::error!(
                "Project owner already exists with the following name: {}",
                data.name
            );

            let html = render_to_string(move || {
                view! {
                    <p> Project with name {data.name} already exists </p>
                }
            })
            .into_owned();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(html))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't get existing project owner: Failed to query database"
            );

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap();
        }
    };

    let owner_id = Uuid::from(Ulid::new());

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't insert project owner: Failed to begin transaction"
            );
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to begin transaction {err.to_string()} </h1>
                }
            })
            .into_owned();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }
    };

    if let Err(err) = sqlx::query!(
        r#"INSERT INTO project_owners (id, name)
        VALUES ($1, $2)
        "#,
        owner_id,
        data.name
    )
    .execute(&mut *tx)
    .await
    {
        tracing::error!(
            ?err,
            "Can't insert project owner: Failed to insert into database"
        );
        if let Err(err) = tx.rollback().await {
            tracing::error!(
                ?err,
                "Can't insert project owner: Failed to rollback transaction"
            );
        }

        let html = render_to_string(move || {
            view! {
                <h1> Failed to insert project owner into database </h1>
            }
        })
        .into_owned();

        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(Body::from(html))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html")
        .body(Body::empty())
        .unwrap()
}
