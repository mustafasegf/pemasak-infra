use hyper::{client::HttpConnector, Body};
use pemasak_infra::{
    configuration,
    queue::{build_queue_handler, BuildQueue},
    startup, telemetry,
};
use sqlx::postgres::PgPoolOptions;
use std::{net::TcpListener, path::Path, process};
use tokio::fs::OpenOptions;

type Client = hyper::client::Client<HttpConnector, Body>;

#[tokio::main]
async fn main() {
    telemetry::init_tracing();
    let config = match configuration::get_configuration() {
        Ok(config) => config,
        Err(err) => {
            tracing::error!(?err, "Failed to read configuration");
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
            tracing::error!(?err, "Failed to connect to Postgres");
            process::exit(1);
        }
    };

    // check if the database is up
    if let Err(err) = sqlx::query("SELECT 1").fetch_one(&pool).await.map(|_| ()) {
        tracing::error!(?err, "Failed to query Postgres");
        process::exit(1);
    }

    // check if atlas_chema_revisions exist
    // TODO: maybe rethink this if we actually want to use this table
    match sqlx::query!(
        r#"SELECT * FROM information_schema.tables 
           WHERE table_schema = 'public' 
           AND table_name = 'atlas_schema_revisions'
        "#
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(_)) => {}
        Ok(None) => {
            let err = "atlas_schema_revisions table not found";
            tracing::error!(err, "Failed to query Postgres");
            process::exit(1);
        }
        Err(err) => {
            tracing::error!(?err, "Failed to query Postgres");
            process::exit(1);
        }
    }

    // check docker permissions
    if let Err(err) = tokio::fs::metadata("/var/run/docker.sock").await {
        tracing::error!(?err, "Failed to access docker socket");
        process::exit(1);
    }

    // check if git folder exists
    match tokio::fs::metadata(&config.git.base).await {
        Err(err) => {
            tracing::error!(?err, "Failed to access git folder");
            process::exit(1);
        }
        Ok(metadata) => {
            if !metadata.is_dir() {
                tracing::error!("Git folder is not a directory");
                process::exit(1);
            }
            if metadata.permissions().readonly() {
                tracing::error!("Git folder is read-only");
                process::exit(1);
            }

            let git_path = Path::new(&config.git.base);
            let temp_path = git_path.join("temp");
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
                .await
            {
                Ok(_) => {
                    // Clean up: remove the temporary file
                    if let Err(err) = tokio::fs::remove_file(&temp_path).await {
                        tracing::error!(?err, "Failed to remove temporary file");
                    }
                }
                Err(err) => {
                    tracing::error!(?err, "Cannot write to the git folder");
                    process::exit(1);
                }
            }
        }
    }

    let (build_queue, build_channel) = BuildQueue::new(config.build.max, pool.clone());

    tokio::spawn(async move {
        build_queue_handler(build_queue).await;
    });

    let state = startup::AppState {
        base: config.git.base.clone(),
        git_auth: config.git.auth,
        sso: config.auth.sso.clone(),
        client: Client::new(),
        domain: config.domain(),
        build_channel,
        pool,
        secure: config.application.secure,
    };

    let addr_string = config.address_string();

    let addr = match config.address() {
        Ok(addr) => addr,
        Err(err) => {
            tracing::error!(?err, "Failed to parse address {}", addr_string);
            process::exit(1);
        }
    };

    let listener = match TcpListener::bind(addr) {
        Ok(listener) => listener,
        Err(err) => {
            tracing::error!(?err, "Failed to bind address {}", addr_string);
            process::exit(1);
        }
    };

    if let Err(err) = startup::run(listener, state, config).await {
        tracing::error!(?err, "Failed to start server on address {}", addr_string);
        process::exit(1);
    };
}
