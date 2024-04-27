use axum::{middleware, Router, routing::{get, post}};
use axum_extra::routing::RouterExt;
use hyper::Body;

use crate::{auth::auth, startup::AppState, configuration::Settings};

mod create_project;
mod project_dashboard;
mod web_terminal;
mod delete_project;
mod delete_volume;
mod preferences;
mod components;
mod view_build_log;

pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route_with_tsr("/api/project/new", get(create_project::get).post(create_project::post))
        .route_with_tsr("/:owner/:project", get(project_dashboard::get))
        .route_with_tsr("/:owner/:project/builds/:build_id", get(view_build_log::get))
        .route_with_tsr("/:owner/:project/preferences", get(preferences::get))
        .route_with_tsr("/:owner/:project/delete", post(delete_project::post))
        .route_with_tsr("/:owner/:project/volume/delete", post(delete_volume::post))
        .route_with_tsr("/:owner/:project/terminal/ws", get(web_terminal::ws))
        .route_with_tsr("/:owner/:project/terminal", get(web_terminal::get))
        .route_layer(middleware::from_fn(auth))
}