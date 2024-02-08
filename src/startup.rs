use axum::extract::{Host, State};
use axum::middleware::Next;
use axum::{middleware, Router};

use axum_session::{SessionLayer, SessionPgPool};
use axum_session_auth::AuthSessionLayer;
use bollard::service::ContainerStateStatusEnum;
use bollard::Docker;
use bytes::Bytes;
use http_body::combinators::UnsyncBoxBody;
use hyper::{Body, Method, Request, Response, StatusCode, Uri};

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::mpsc::Sender;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use uuid::Uuid;

use anyhow::Result;
use std::net::{SocketAddr, TcpListener};

use crate::auth::User;
use crate::configuration::Settings;
use crate::docker::start_container;
use crate::queue::BuildQueueItem;
use crate::{auth, dashboard, git, owner, projects, telemetry};

#[derive(Clone)]
pub struct AppState {
    pub base: String,
    pub git_auth: bool,
    pub sso: bool,
    pub register: bool,
    pub domain: String,
    pub host_ip: String,
    pub client: hyper::client::Client<hyper::client::HttpConnector, hyper::Body>,
    pub pool: PgPool,
    pub build_channel: Sender<BuildQueueItem>,
    pub secure: bool,
    pub idle_channel: Sender<String>,
}

#[derive(Default, Clone, Serialize, Deserialize, Debug, sqlx::Type, strum::Display)]
#[sqlx(type_name = "build_state", rename_all = "lowercase")]
pub enum ProjectState {
    #[default]
    Empty,
    Running,
    Stopped,
    Idle,
}

pub async fn run(listener: TcpListener, state: AppState, config: Settings) -> Result<(), String> {
    tokio::spawn({
        let pool = state.pool.clone();
        async move {
            check_build(&pool).await;
            start_docker_container(&pool).await.unwrap();
        }
    });

    let http_trace = telemetry::http_trace_layer();
    let pool = state.pool.clone();

    let (auth_config, session_store) = auth::auth_layer(&pool, &config).await;

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
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

/// check if there's still docker that's stuck on building and make them failed
pub async fn check_build(pool: &PgPool) {
    match sqlx::query!(
        r#"
        update builds
        set status = 'failed', log = 'Server restarted while building. Please try again.'
        WHERE status = 'building'
        returning id, (select name from projects where id = project_id)
        "#
    )
    .fetch_all(pool)
    .await
    {
        Err(err) => tracing::error!(?err, "Failed to check build status"),
        Ok(records) => {
            for record in records {
                tracing::info!(
                    ?record,
                    "Build {}, project {} is stuck, marking it as failed",
                    record.id,
                    record.name.clone().unwrap_or_default()
                );
            }
        }
    }
}

/// get all project and check if docker is running. run docker if it's not running
pub async fn start_docker_container(pool: &PgPool) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;
    let container_names = sqlx::query!(
        r#"
            SELECT domains.name
            FROM domains 
            JOIN projects on domains.project_id = projects.id 
            WHERE projects.state = 'running'
        "#
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|record| record.name)
    .collect::<Vec<String>>();

    for container_name in container_names {
        if let Ok(container) = docker.inspect_container(&container_name, None).await {
            if container
                .state
                .and_then(|state| state.status)
                .and_then(|status| {
                    (status == ContainerStateStatusEnum::EXITED
                        || status == ContainerStateStatusEnum::DEAD)
                        .then_some(())
                })
                .is_some()
            {
                tracing::info!("Starting container {}", container_name);
                let db_name = format!("{}-db", container_name);

                if let Err(err) = start_container(&docker, &db_name, true).await {
                    tracing::error!(?err, "Can't start container: Failed to start container");
                }

                if let Err(err) = start_container(&docker, &container_name, false).await {
                    tracing::error!(?err, "Can't start container: Failed to start container");
                }
            }
        };
    }

    Ok(())
}

pub async fn fallback(
    State(AppState {
        pool,
        client,
        domain,
        idle_channel,
        host_ip,
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

    if let Err(err) = idle_channel.send(subdomain.to_string()).await {
        tracing::error!(?err, "Failed to send idle channel");
    }

    let (owner, project) = match subdomain.rfind('-') {
        Some(index) => (
            subdomain[..index].replace('-', "."),
            &subdomain[index + 1..],
        ),
        None => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap()
        }
    };

    let project_record = match sqlx::query!(
        r#"
        SELECT projects.id, projects.state as "state: ProjectState", domains.host_ip
        FROM projects
        JOIN project_owners ON projects.owner_id = project_owners.id
        JOIN users_owners ON project_owners.id = users_owners.owner_id
        JOIN domains on projects.id = domains.project_id
        AND projects.name = $1
        AND project_owners.name = $2
      "#,
        project,
        owner,
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(record)) => record,
        Ok(None) => {
            tracing::debug!("Can't get project: Project does not exist");
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get project_owners: Failed to query database");

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap();
        }
    };

    if project_record.host_ip != host_ip && project_record.host_ip != "0.0.0.0" {
        let uri = format!("http://{}{}", project_record.host_ip, uri);
        *req.uri_mut() = Uri::try_from(uri).unwrap();
        match client.request(req).await {
            Ok(res) => {
                tracing::debug!("response");
                return res;
            }
            Err(err) => {
                tracing::error!(?err, "Can't access container: Failed request to container");

                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::empty())
                    .unwrap();
            }
        }
    }

    let db_name = format!("{subdomain}-db");
    let docker = Docker::connect_with_local_defaults().unwrap();

    match project_record.state {
        ProjectState::Stopped => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap()
        }
        ProjectState::Idle => {
            if let Err(err) = start_container(&docker, &db_name, true).await {
                tracing::error!(?err, "Can't start container: Failed to start container");
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::empty())
                    .unwrap();
            }
            if let Err(err) = start_container(&docker, subdomain, false).await {
                tracing::error!(?err, "Can't start container: Failed to start container");
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::empty())
                    .unwrap();
            }

            if let Err(err) = sqlx::query!(
                r#"
                    UPDATE projects
                    SET state = 'running'
                    WHERE id = $1
                "#,
                project_record.id
            )
            .execute(&pool)
            .await
            {
                tracing::error!(?err, "Can't update project: Failed to update project");
            }
        }
        _ => {}
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
                Ok(res) => {
                    tracing::debug!("response");
                    res
                }
                Err(err) if err.is_connect() => {
                    // check the ip for the container
                    // TODO: remove unwrap
                    let docker = Docker::connect_with_local_defaults().unwrap();
                    let container = docker.inspect_container(subdomain, None).await.unwrap();
                    let ip = match container
                        .network_settings
                        .and_then(|network| network.networks)
                        .and_then(|networks| {
                            networks
                                .get(format!("{}-network", subdomain).as_str())
                                .cloned()
                        })
                        .and_then(|network| network.ipam_config)
                        .and_then(|ipam| ipam.ipv4_address)
                    {
                        Some(ip) => ip,
                        None => {
                            tracing::error!("Can't get ip: Failed to get ip");
                            return Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::empty())
                                .unwrap();
                        }
                    };

                    if let Err(err) = sqlx::query!(
                        r#"
                                UPDATE domains
                                SET docker_ip = $1
                                WHERE name = $2
                            "#,
                        ip,
                        subdomain
                    )
                    .execute(&pool)
                    .await
                    {
                        tracing::error!(?err, "Can't update domain: Failed to update domain");
                        return Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(Body::empty())
                            .unwrap();
                    };

                    Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap()
                }
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
        idle_channel,
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

    if let Err(err) = idle_channel.send(subdomain.to_string()).await {
        tracing::error!(?err, "Failed to send idle channel");
    }

    let (owner, project) = match subdomain.rfind('-') {
        Some(index) => (
            subdomain[..index].replace('-', "."),
            &subdomain[index + 1..],
        ),
        None => {
            return Err(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap())
        }
    };

    let project_record = match sqlx::query!(
        r#"
        SELECT projects.id, projects.state as "state: ProjectState"
        FROM projects
        JOIN project_owners ON projects.owner_id = project_owners.id
        JOIN users_owners ON project_owners.id = users_owners.owner_id
        AND projects.name = $1
        AND project_owners.name = $2
      "#,
        project,
        owner,
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(record)) => record,
        Ok(None) => {
            tracing::debug!("Can't get project: Project does not exist");
            return Err(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap());
        }
        Err(err) => {
            tracing::error!(?err, "Can't get project_owners: Failed to query database");

            return Err(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap());
        }
    };

    let db_name = format!("{subdomain}-db");
    let docker = Docker::connect_with_local_defaults().unwrap();

    match project_record.state {
        ProjectState::Stopped => {
            return Err(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap())
        }
        ProjectState::Idle => {
            if let Err(err) = start_container(&docker, &db_name, true).await {
                tracing::error!(?err, "Can't start container: Failed to start container");
                return Err(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::empty())
                    .unwrap());
            }
            if let Err(err) = start_container(&docker, subdomain, false).await {
                tracing::error!(?err, "Can't start container: Failed to start container");
                return Err(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::empty())
                    .unwrap());
            }

            if let Err(err) = sqlx::query!(
                r#"
                    UPDATE projects
                    SET state = 'running'
                    WHERE id = $1
                "#,
                project_record.id
            )
            .execute(&pool)
            .await
            {
                tracing::error!(?err, "Can't update project: Failed to update project");
            }
        }
        _ => {}
    }

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
                Err(err) if err.is_connect() => {
                    // check the ip for the container
                    // TODO: remove unwrap
                    let docker = Docker::connect_with_local_defaults().unwrap();
                    let container = docker.inspect_container(subdomain, None).await.unwrap();
                    let ip = match container
                        .network_settings
                        .and_then(|network| network.networks)
                        .and_then(|networks| {
                            networks
                                .get(format!("{}-network", subdomain).as_str())
                                .cloned()
                        })
                        .and_then(|network| network.ipam_config)
                        .and_then(|ipam| ipam.ipv4_address)
                    {
                        Some(ip) => ip,
                        None => {
                            tracing::error!("Can't get ip: Failed to get ip");
                            return Err(Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::empty())
                                .unwrap());
                        }
                    };

                    if let Err(err) = sqlx::query!(
                        r#"
                            UPDATE domains
                            SET docker_ip = $1
                            WHERE name = $2
                        "#,
                        ip,
                        subdomain
                    )
                    .execute(&pool)
                    .await
                    {
                        tracing::error!(?err, "Can't update domain: Failed to update domain");
                        return Err(Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .body(Body::empty())
                            .unwrap());
                    };

                    return Err(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap());
                }
                Err(err) => {
                    tracing::error!(?err, "Can't access container: Failed request to container");

                    // update to new ip

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
