use axum::extract::{Host, State};
use axum::Router;

use axum_session::{SessionLayer, SessionPgPool};
use axum_session_auth::AuthSessionLayer;
use hyper::{Body, Method, Request, Response, StatusCode, Uri};

use sqlx::PgPool;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use std::net::TcpListener;

use crate::auth::User;
use crate::configuration::Settings;
use crate::{auth, git, telemetry, projects};

#[derive(Clone)]
pub struct AppState {
    pub base: String,
    pub auth: bool,
    pub domain: String,
    pub client: hyper::client::Client<hyper::client::HttpConnector, hyper::Body>,
    pub pool: PgPool,
}

pub async fn run(listener: TcpListener, state: AppState, config: Settings) -> Result<(), String> {
    let http_trace = telemetry::http_trace_layer();
    let pool = state.pool.clone();

    let (auth_config, session_store) = auth::auth_layer(&pool).await;

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_origin(Any);

    let git_router = git::router(state.clone(), &config);
    let auth_router = auth::router(state.clone(), &config).await;
    let project_router = projects::router(state.clone(), &config).await;

    let app = Router::new()
        .merge(git_router)
        .merge(auth_router)
        .merge(project_router)
        .layer(http_trace)
        .fallback(fallback)
        .layer(
            AuthSessionLayer::<User, Uuid, SessionPgPool, PgPool>::new(Some(pool.clone()))
                .with_config(auth_config),
        )
        .layer(SessionLayer::new(session_store))
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

// TODO: use db
pub async fn fallback(
    State(AppState { client, domain, .. }): State<AppState>,
    Host(hostname): Host,
    uri: axum::http::Uri,
    mut req: Request<Body>,
) -> Response<Body> {
    let sub_domain = hostname
        .trim_end_matches(domain.as_str())
        .trim_end_matches('.');

    tracing::info!(hostname, sub_domain);

    if sub_domain.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .unwrap();
    }

    // let map = REGISTERED_ROUTES.read().unwrap();
    // let route = map.get(sub_domain);
    let route = Some("172.31.0.2:80".to_string());

    match route {
        Some(route) => {
            let uri = format!("http://{}{}", route, uri);
            *req.uri_mut() = Uri::try_from(uri).unwrap();
            client.request(req).await.unwrap()
        }
        None => {
            tracing::debug!("route not found uri -> {:#?}", uri);

            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap()
        }
    }
}
