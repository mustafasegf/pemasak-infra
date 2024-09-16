use axum::{extract::{Path, State}, response::Response};
use hyper::{Body, StatusCode};
use leptos::{ssr::render_to_string, view};
use uuid::Uuid;

use crate::{auth::Auth, startup::AppState};

#[tracing::instrument(skip(auth, pool))]
pub async fn post(
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
