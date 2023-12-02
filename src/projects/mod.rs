use axum::{middleware, Router, routing::{get, post}};
use axum_extra::routing::RouterExt;
use hyper::Body;

use crate::{auth::auth, startup::AppState, configuration::Settings};

mod create_project;
mod dashboard;
mod project_dashboard;
mod web_terminal;
mod delete_project;
mod delete_volume;

pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route_with_tsr("/new", get(create_project::get).post(create_project::post))
        .route_with_tsr("/dashboard", get(dashboard::get).post(create_project::post))
        .route_with_tsr("/:owner/:project", get(project_dashboard::get))
        .route_with_tsr("/:owner/:project/delete", post(delete_project::post))
        .route_with_tsr("/:owner/:project/volume/delete", post(delete_volume::post))
        .route_with_tsr("/:owner/:project/terminal/ws", get(web_terminal::ws))
        .route_with_tsr("/:owner/:project/terminal", get(web_terminal::get))
        .route_layer(middleware::from_fn(auth))
}