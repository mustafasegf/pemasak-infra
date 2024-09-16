use axum::{middleware, routing::post, Router};
use axum_extra::routing::RouterExt;
use hyper::Body;

use crate::{auth::auth, configuration::Settings, startup::AppState};

mod create_project_owner;
mod update_project_owner;
mod invite_project_member;
mod remove_project_member;

pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route_with_tsr(
            "/owner",
            post(create_project_owner::post),
        )
        .route_with_tsr(
            "/owner/:owner_id",
            post(update_project_owner::post),
        )
        .route_with_tsr(
            "/owner/:owner_id/invite",
            post(invite_project_member::post),
        )
        .route_layer(middleware::from_fn(auth))
}
