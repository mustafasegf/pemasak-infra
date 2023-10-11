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
use crate::{auth, git, projects, telemetry};

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
    State(AppState {
        pool,
        client,
        domain,
        ..
    }): State<AppState>,
    Host(hostname): Host,
    uri: axum::http::Uri,
    mut req: Request<Body>,
) -> Response<Body> {
    let subdomain = hostname
        .trim_end_matches(domain.as_str())
        .trim_end_matches('.');

    if subdomain.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .unwrap();
    }

    tracing::info!(subdomain);

    match sqlx::query!(
        r#"SELECT docker_ip, port
           FROM domains
           WHERE name = $1
        "#,
        subdomain
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(route)) => {
            let uri = format!("http://{}:{}{}", route.docker_ip, route.port, uri);
            *req.uri_mut() = Uri::try_from(uri).unwrap();
            client.request(req).await.unwrap()
        }
        Ok(None) => {
            tracing::debug!("route not found uri -> {:#?}", uri);

            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap()
        }
        Err(err) => {
            tracing::error!("route not found uri -> {:#?}", err);

            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap()
        }
    }
}
