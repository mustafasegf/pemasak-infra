use axum::Router;
use axum_extra::routing::RouterExt;
use hyper::Body;
use tower_http::services::ServeDir;

use crate::{components::Base, configuration::Settings, startup::AppState};

pub async fn router(state: AppState, config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route_service("/web/login", ServeDir::new("ui/dist"))
        .route_service("/web/register", ServeDir::new("ui/dist"))
        .route_with_tsr("/web/*path", get(validate_auth))
}