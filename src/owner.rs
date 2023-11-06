use axum::{
    extract::{Path, State},
    response::Response,
    routing::post,
    Form, Router,
};
use garde::{Unvalidated, Validate};
use hyper::{Body, StatusCode};
use leptos::ssr::render_to_string;
use leptos::*;
use serde::Deserialize;
use ulid::Ulid;
use uuid::Uuid;

use crate::{auth::Auth, configuration::Settings, startup::AppState};

// TODO: separate schema for create and update when needed later on
#[derive(Deserialize, Validate, Debug)]
pub struct OwnerRequest {
    #[garde(length(max = 128))]
    pub name: String,
}

#[tracing::instrument(skip(auth, pool))]
pub async fn remove_project_member(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Path((owner_id, user_id)): Path<(Uuid, Uuid)>,
) -> Response<Body> {
    let authed_user_id = auth.id;

    // Check if requesting user is already in owner group
    match sqlx::query!(
        r#"SELECT user_id, owner_id FROM users_owners
        WHERE user_id = $1 AND owner_id = $2
        "#,
        authed_user_id,
        owner_id
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(_)) => (),
        Ok(None) => {
            tracing::error!(
                "Can't find existing user_owner with user_id {} and owner_id {}",
                user_id,
                owner_id,
            );

            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::empty())
                .unwrap();
        }
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't get existing user_owner: Failed to query database"
            );

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        }
    }

    if let Err(err) = sqlx::query!(
        r#"DELETE FROM users_owners
        WHERE user_id = $1 AND owner_id = $2"#,
        user_id,
        owner_id
    )
    .execute(&pool)
    .await
    {
        tracing::error!(
            ?err,
            "Can't delete users_owners: Failed to insert into database"
        );

        let html = render_to_string(move || {
            view! {
                <h1> Failed to remove owner group member </h1>
            }
        })
        .into_owned();

        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(Body::from(html))
            .unwrap();
    };

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap()
}

#[tracing::instrument(skip(auth, pool))]
pub async fn invite_project_member(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Path((owner_id, user_id)): Path<(Uuid, Uuid)>,
) -> Response<Body> {
    let authed_user_id = auth.id;

    // Check if requesting user is already in owner group
    match sqlx::query!(
        r#"SELECT user_id, owner_id FROM users_owners
        WHERE user_id = $1 AND owner_id = $2
        "#,
        authed_user_id,
        owner_id
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(_)) => (),
        Ok(None) => {
            tracing::error!(
                "Can't find existing user_owner with user_id {} and owner_id {}",
                user_id,
                owner_id,
            );

            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::empty())
                .unwrap();
        }
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't get existing user_owner: Failed to query database"
            );

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        }
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't insert users_owners: Failed to begin transaction"
            );
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to begin transaction {err.to_string()} </h1>
                }
            })
            .into_owned();

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }
    };

    if let Err(err) = sqlx::query!(
        r#"INSERT INTO users_owners (user_id, owner_id)
        VALUES ($1, $2)"#,
        user_id,
        owner_id,
    )
    .execute(&mut *tx)
    .await
    {
        tracing::error!(
            ?err,
            "Can't insert users_owners: Failed to insert into database"
        );

        let error_html = match err {
            sqlx::Error::Database(err) => {
                if err.constraint() == Some("users_owners_pkey") {
                    render_to_string(move || {
                        view! {
                            <h1> User is already in the owner group </h1>
                        }
                    })
                    .into_owned()
                } else {
                    render_to_string(move || {
                        view! {
                            <h1> Failed to invite member to owner group </h1>
                        }
                    })
                    .into_owned()
                }
            }
            _ => render_to_string(move || {
                view! {
                    <h1> Failed to invite member to owner group </h1>
                }
            })
            .into_owned(),
        };

        if let Err(err) = tx.rollback().await {
            tracing::error!(
                ?err,
                "Can't insert users_owners: Failed to rollback transaction"
            );
        }

        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(Body::from(error_html))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap()
}

#[tracing::instrument(skip(_auth, pool))]
pub async fn create_project_owner(
    _auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Form(req): Form<Unvalidated<OwnerRequest>>,
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

#[tracing::instrument()]
pub async fn update_project_owner(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Path(owner_id): Path<String>,
    Form(req): Form<Unvalidated<OwnerRequest>>,
) -> Response<Body> {
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap()
}

pub fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route("/owner", post(create_project_owner))
        .route("/owner/:owner_id", post(update_project_owner))
        .route(
            "/owner/:owner_id/:user_id",
            post(invite_project_member).delete(remove_project_member),
        )
}
