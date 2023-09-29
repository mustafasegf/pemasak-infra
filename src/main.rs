use anyhow::Result;
use hyper::{client::HttpConnector, Body, Request, StatusCode, Uri};
use pemasak_infra::{
    configuration,
    git::{
        get_file_text, get_info_packs, get_info_refs, get_loose_object, get_pack_or_idx_file,
        recieve_pack_rpc, upload_pack_rpc,
    },
    startup, telemetry,
};
use std::{
    net::{SocketAddr, TcpListener},
    process,
};

type Client = hyper::client::Client<HttpConnector, Body>;

use axum::{
    extract::{DefaultBodyLimit, Host, Path, State},
    response::Response,
    routing::{get, post},
    Router,
};

pub async fn fallback(
    State(client): State<Client>,
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

// #[tokio::main]
// async fn main() -> Result<()> {
//     // let _container_name = "go-example".to_string();
//     // let _image_name = "go-example:latest".to_string();
//     // let _container_src = "./src/go-example".to_string();
//     // let _network_name = "go-example-network".to_string();
//     //
//     // let git_repo_path = "./src/git-repo".to_string();
//     // let git_repo_name = "mustafa.git".to_string();
//     //
//     // let _full_repo_path = format!("{}/{}", git_repo_path, git_repo_name);
//
//     let client = Client::new();
//
//     let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
//     let app = Router::new()
//         .route("/:repo/git-upload-pack", post(upload_pack_rpc))
//         .route("/:repo/git-receive-pack", post(recieve_pack_rpc))
//         .route("/:repo/info/refs", get(get_info_refs))
//         .route(
//             "/:repo/HEAD",
//             get(|Path(repo): Path<String>| async move { get_file_text(repo, "HEAD").await }),
//         )
//         .route(
//             "/:repo/objects/info/alternates",
//             get(|Path(repo): Path<String>| async move {
//                 get_file_text(repo, "objects/info/alternates").await
//             }),
//         )
//         .route(
//             "/:repo/objects/info/http-alternates",
//             get(|Path(repo): Path<String>| async move {
//                 get_file_text(repo, "objects/info/http-alternates").await
//             }),
//         )
//         .route("/:repo/objects/info/packs", get(get_info_packs))
//         .route(
//             "/:repo/objects/info/:file",
//             get(
//                 |Path((repo, head, file)): Path<(String, String, String)>| async move {
//                     get_file_text(repo, format!("{}/{}", head, file).as_ref()).await
//                 },
//             ),
//         )
//         .route("/:repo/objects/:head/:hash", get(get_loose_object))
//         .route("/:repo/objects/packs/:file", get(get_pack_or_idx_file))
//         .layer(DefaultBodyLimit::disable())
//         .fallback(fallback)
//         .with_state(client);
//
//     axum::Server::bind(&addr)
//         .serve(app.into_make_service())
//         .await
//         .unwrap();
//
//     Ok(())
// }

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
        secret: config.application.secret.clone(),
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
