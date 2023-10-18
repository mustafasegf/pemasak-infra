use hyper::{client::HttpConnector, Body};
use pemasak_infra::{configuration, startup, telemetry, queue::{BuildQueue, build_queue_handler}};
use sqlx::postgres::PgPoolOptions;
use std::{net::TcpListener, process};

type Client = hyper::client::Client<HttpConnector, Body>;

#[tokio::main]
async fn main() {
    telemetry::init_tracing();
    let config = match configuration::get_configuration() {
        Ok(config) => config,
        Err(err) => {
            tracing::error!("Failed to read configuration: {}", err);
            process::exit(1);
        }
    };

    let pool = match PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(config.database.timeout))
        .connect_with(config.connection_options())
        .await
    {
        Ok(pool) => pool,
        Err(err) => {
            tracing::error!("Failed to connect to Postgres: {}", err);
            process::exit(1);
        }
    };

    // check if the database is up
    if let Err(err) = sqlx::query("SELECT 1")
        .fetch_one(&pool)
        .await
        .map(|_| ())
    {
        tracing::error!("Failed to query Postgres: {}", err);
        process::exit(1);
    }

    // check if atlas_chema_revisions exist
    // TODO: maybe rethink this if we actually want to use this table
    match sqlx::query(r#"SELECT * FROM information_schema.tables 
                         WHERE table_schema = 'public' 
                         AND table_name = 'atlas_schema_revisions'"#)
        .fetch_one(&pool)
        .await
        .map(|_| ())
    { 
        Ok(_) => {},
        Err(sqlx::Error::RowNotFound) => {
            tracing::error!("Failed to query Postgres: atlas_schema_revisions table not found");
            process::exit(1);
        },
        Err(err) => {
            tracing::error!("Failed to query Postgres: {}", err);
            process::exit(1);
        }
    }

    // check docker permissions
    if let Err(err) = tokio::fs::metadata("/var/run/docker.sock").await {
        tracing::error!("Failed to access docker socket: {}", err);
        process::exit(1);
    }

    let (build_queue, build_channel) = BuildQueue::new(1, pool.clone());
    
    tokio::spawn(async move {
        build_queue_handler(build_queue).await;
    });

    let state = startup::AppState {
        base: config.git.base.clone(),
        auth: config.application.auth,
        client: Client::new(),
        domain: config.domain(),
        build_channel,
        pool,
    };

    let addr_string = config.address_string();

    let addr = match config.address() {
        Ok(addr) => addr,
        Err(err) => {
            tracing::error!("Failed to parse address {}: {}", addr_string, err);
            process::exit(1);
        }
    };

    let listener = match TcpListener::bind(addr) {
        Ok(listener) => listener,
        Err(err) => {
            tracing::error!("Failed to bind address {}: {}", addr_string, err);
            process::exit(1);
        }
    };

    if let Err(err) = startup::run(listener, state, config).await {
        tracing::error!("Failed to start server on address {}: {}", addr_string, err);
        process::exit(1);
    };
}
