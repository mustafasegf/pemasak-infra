use axum::extract::{Host, State};
use axum::Router;

use hyper::{Body, Method, Request, Response, StatusCode, Uri};

use tower_http::cors::{Any, CorsLayer};

use std::net::TcpListener;

use crate::configuration::Settings;
use crate::{git, telemetry};

#[derive(Clone)]
pub struct AppState {
    pub base: String,
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
        .fallback(fallback)
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
    State(AppState { client, .. }): State<AppState>,
    Host(hostname): Host,
    uri: axum::http::Uri,
    mut req: Request<Body>,
) -> Response<Body> {
    let domain = "localhost:3000";
    let sub_domain = hostname.trim_end_matches(domain).trim_end_matches(".");

    println!("hostname -> {:#?}", hostname);
    println!("sub_hostname -> {:#?}", sub_domain);

    // let map = REGISTERED_ROUTES.read().unwrap();
    // let route = map.get(sub_domain);
    let route = Some("172.31.0.2:8080".to_string());

    match route {
        // Some(route) => (axum::http::StatusCode::OK, route.to_string()),
        Some(route) => {
            let uri = format!("http://{}{}", route, uri);
            println!("uri -> {:#?}", uri);

            *req.uri_mut() = Uri::try_from(uri).unwrap();
            let res = client.request(req).await.unwrap();
            res
        }
        None => {
            println!("route not found uri -> {:#?}", uri);
            println!("hostname -> {:#?}", hostname);
            println!("sub_hostname -> {:#?}", sub_domain);

            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap()
        }
    }
}
