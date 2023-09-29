use hyper::{client::HttpConnector, Body};
use pemasak_infra::{configuration, startup, telemetry};
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

    // let pool = match PgPoolOptions::new()
    //     .acquire_timeout(std::time::Duration::from_secs(config.database.timeout))
    //     .connect_with(config.connection_options())
    //     .await
    // {
    //     Ok(pool) => pool,
    //     Err(err) => {
    //         tracing::error!("Failed to connect to Postgres: {}", err);
    //         process::exit(1);
    //     }
    // };
    //

    let state = startup::AppState {
        base: config.git.base.clone(),
        auth: config.application.auth,
        client: Client::new(),
        // pool,
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

    match startup::run(listener, state, config).await {
        Err(err) => {
            tracing::error!("Failed to start server on address {}: {}", addr_string, err);
            process::exit(1);
        }
        _ => {}
    };
}
