use axum::extract::{Host, State};
use axum::middleware::Next;
use axum::{middleware, Router};

use axum_session::{SessionLayer, SessionPgPool};
use axum_session_auth::AuthSessionLayer;
use bytes::Bytes;
use http_body::combinators::UnsyncBoxBody;
use hyper::{Body, Method, Request, Response, StatusCode, Uri};

use sqlx::PgPool;
use tokio::sync::mpsc::Sender;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use uuid::Uuid;

use std::net::{SocketAddr, TcpListener};

use crate::auth::User;
use crate::configuration::Settings;
use crate::queue::BuildQueueItem;
use crate::{auth, dashboard, git, owner, projects, telemetry};

#[derive(Clone)]
pub struct AppState {
    pub base: String,
    pub git_auth: bool,
    pub sso: bool,
    pub domain: String,
    pub client: hyper::client::Client<hyper::client::HttpConnector, hyper::Body>,
    pub pool: PgPool,
    pub build_channel: Sender<BuildQueueItem>,
    pub secure: bool,
}

pub async fn run(listener: TcpListener, state: AppState, config: Settings) -> Result<(), String> {
    let http_trace = telemetry::http_trace_layer();
    let pool = state.pool.clone();

    let (auth_config, session_store) = auth::auth_layer(&pool, &config).await;

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any)
        .allow_origin(Any);

    let git_router = git::router(state.clone(), &config);
    let auth_router = auth::router(state.clone(), &config).await;
    let dashboard_router: Router<AppState> = dashboard::router(state.clone(), &config).await;
    let project_router = projects::router(state.clone(), &config).await;
    let owners_router = owner::router(state.clone(), &config).await;

    let app = Router::new()
        .merge(git_router)
        .merge(auth_router)
        .merge(dashboard_router)
        .merge(project_router)
        .merge(owners_router)
        .layer(http_trace)
        // TODO: rethink if we need this here. since it makes all routes under this query the
        // session even if they don't need it
        .layer(
            AuthSessionLayer::<User, Uuid, SessionPgPool, PgPool>::new(Some(pool.clone()))
                .with_config(auth_config),
        )
        .layer(SessionLayer::new(session_store))
        .nest_service("/assets", ServeDir::new("assets"))
        .route(
            "/",
            axum::routing::get(|| async { axum::response::Redirect::temporary("/dashboard") }),
        )
        .fallback(fallback)
        .with_state(state.clone())
        .route_layer(middleware::from_fn_with_state(state, fallback_middleware))
        .layer(cors);

    let addr = listener
        .local_addr()
        .map_err(|err| format!("Failed to get local address: {}", err))?;

    tracing::info!("listening on {}", addr);

    axum::Server::from_tcp(listener)
        .map_err(|err| format!("Failed to make server from tcp: {}", err))?
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .map_err(|err| format!("failed to start server: {}", err))
}

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
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    }

    tracing::debug!(hostname, "hostname {}", hostname);
    tracing::debug!(domain, "domain {}", domain);
    tracing::debug!(?subdomain, "subdomain {} is accessed", subdomain);

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
            tracing::debug!(
                ip = route.docker_ip,
                port = route.port,
                ?uri,
                "route found {}",
                uri
            );
            let uri = format!("http://{}:{}{}", route.docker_ip, route.port, uri);
            *req.uri_mut() = Uri::try_from(uri).unwrap();
            match client.request(req).await {
                Ok(res) => res,
                Err(err) => {
                    tracing::error!(?err, "Can't access container: Failed request to container");

                    Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap()
                }
            }
        }
        Ok(None) => {
            tracing::debug!(?uri, "route not found {}", uri);

            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap()
        }
        Err(err) => {
            tracing::error!(?err, "Can't get subdomain: Failed to query database");

            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap()
        }
    }
}

pub async fn fallback_middleware(
    State(AppState {
        pool,
        client,
        domain,
        ..
    }): State<AppState>,
    Host(hostname): Host,
    uri: axum::http::Uri,
    mut req: Request<Body>,
    next: Next<Body>,
) -> Result<Response<UnsyncBoxBody<Bytes, axum::Error>>, Response<Body>> {
    let subdomain = hostname
        .trim_end_matches(domain.as_str())
        .trim_end_matches('.');

    tracing::debug!(hostname, "hostname {}", hostname);
    tracing::debug!(domain, "domain {}", domain);
    tracing::debug!(?subdomain, "subdomain {} is accessed", subdomain);

    if subdomain.is_empty() {
        return Ok(next.run(req).await);
    }

    tracing::debug!(?subdomain, "subdomain {} is accessed", subdomain);

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
            tracing::debug!(
                ip = route.docker_ip,
                port = route.port,
                ?uri,
                "route found {}",
                uri
            );
            let uri = format!("http://{}:{}{}", route.docker_ip, route.port, uri);
            *req.uri_mut() = Uri::try_from(uri).unwrap();
            match client.request(req).await {
                Ok(res) => Err(res),
                Err(err) => {
                    tracing::error!(?err, "Can't access container: Failed request to container");

                    Err(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap())
                }
            }
        }
        Ok(None) => {
            tracing::debug!(?uri, "route not found {}", uri);

            Err(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap())
        }
        Err(err) => {
            tracing::error!(?err, "Can't get subdomain: Failed to query database");

            Err(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap())
        }
    }
}
