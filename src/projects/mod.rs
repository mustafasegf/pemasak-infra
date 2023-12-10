use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use axum_extra::routing::RouterExt;
use hyper::Body;

use crate::{auth::auth, configuration::Settings, startup::AppState};

mod components;
mod create_project;
mod delete_project;
mod delete_volume;
mod preferences;
mod project_dashboard;
mod view_build_log;
mod web_terminal;
mod logs;

pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route_with_tsr("/new", get(create_project::get).post(create_project::post))
        .route_with_tsr("/:owner/:project", get(project_dashboard::get))
        .route_with_tsr(
            "/:owner/:project/builds/:build_id",
            get(view_build_log::get),
        )
        .route_with_tsr("/:owner/:project/preferences", get(preferences::get))
        .route_with_tsr("/:owner/:project/delete", post(delete_project::post))
        .route_with_tsr("/:owner/:project/volume/delete", post(delete_volume::post))
        .route_with_tsr("/:owner/:project/terminal/ws", get(web_terminal::ws))
        .route_with_tsr("/:owner/:project/terminal", get(web_terminal::get))
        .route_with_tsr("/:owner/:project/logs", get(logs::get))
        .route_layer(middleware::from_fn(auth))
}
