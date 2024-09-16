use axum::{extract::State, response::Response, Form};
use garde::{Unvalidated, Validate};
use hyper::{Body, StatusCode};
use leptos::{ssr::render_to_string, view, IntoView};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::Auth,
    startup::AppState,
};

#[derive(Deserialize, Validate, Debug)]
pub struct InviteRequest {
    #[garde(required)]
    pub owner_id: Option<Uuid>,
    #[garde(required)]
    pub username: Option<String>,
}

#[tracing::instrument(skip(auth, pool))]
pub async fn post(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Form(req): Form<Unvalidated<InviteRequest>>,
) -> Response<Body> {
    let authed_user_id = auth.id;
    let validated_request = match req.validate(&()) {
        Ok(validated_request) => validated_request.into_inner(),
        Err(_err) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from("Invalid request"))
                .unwrap();
        }
    };

    let owner_id = validated_request.owner_id.unwrap();
    let invited_username = validated_request.username.unwrap();

    // Check if requesting user is already in owner group
    match sqlx::query!(
        r#"SELECT user_id, owner_id FROM users_owners
        WHERE user_id = $1 AND owner_id = $2
        "#,
        authed_user_id,
        owner_id,
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(_)) => (),
        Ok(None) => {
            tracing::error!(
                "Can't find existing user_owner with user_id {} and owner_id {}",
                authed_user_id,
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

    let invited_user = match sqlx::query!(
        r#"SELECT id FROM users WHERE username = $1"#,
        invited_username
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(invited_user)) => invited_user.id,
        Ok(None) => {
            tracing::error!(
                "Can't get existing user: User not found with username {}",
                invited_username
            );

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(format!(
                    "User not found with username {}",
                    invited_username
                )))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get existing user: Failed to query database",);

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        }
    };

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
        invited_user,
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

    if let Err(err) = tx.commit().await {
        tracing::error!(
            ?err,
            "Can't create users_owners: Failed to commit transaction"
        );
        let html = render_to_string(move || {
            view! {
                <h1> Failed to commit transaction {err.to_string() } </h1>
            }
        })
        .into_owned();
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(html))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap()
}
