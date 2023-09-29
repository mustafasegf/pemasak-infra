use axum::{middleware, Router};

use hyper::Method;
use sqlx::PgPool;

use tower_http::cors::{Any, CorsLayer};

use std::net::TcpListener;

use crate::{configuration::Settings};
use crate::{git, telemetry};

#[derive(Clone)]
pub struct AppState {
    pub secret: String,
    pub auth: bool,
    pub client: hyper::client::Client<hyper::client::HttpConnector, hyper::Body>,
    // pub pool: PgPool,
}

pub async fn run(listener: TcpListener, state: AppState, config: Settings) -> Result<(), String> {
    let http_trace = telemetry::http_trace_layer();

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_origin(Any);

    let git_router = git::router(state.clone(), &config);

    let app = Router::new()
        .merge(git_router)
        .layer(http_trace)
        .with_state(state)
        .layer(cors);

    let addr = listener
        .local_addr()
        .map_err(|err| format!("Failed to get local address: {}", err))?;

    tracing::info!("listening on {}", addr);

    axum::Server::from_tcp(listener)
        .map_err(|err| format!("Failed to make server from tcp: {}", err))?
        .serve(app.into_make_service())
        .await
        .map_err(|err| format!("failed to start server: {}", err))
}
