// #![allow(dead_code, unused_imports)]

use anyhow::Result;
use hyper::{client::HttpConnector, Body, Request, StatusCode, Uri};
use pemasak_infra::{
    docker::REGISTERED_ROUTES,
    git::{get_info_refs, recieve_pack_rpc, upload_pack_rpc},
};
use std::net::SocketAddr;

type Client = hyper::client::Client<HttpConnector, Body>;

use axum::{
    extract::{DefaultBodyLimit, Host, State},
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

#[tokio::main]
async fn main() -> Result<()> {
    // let _container_name = "go-example".to_string();
    // let _image_name = "go-example:latest".to_string();
    // let _container_src = "./src/go-example".to_string();
    // let _network_name = "go-example-network".to_string();
    //
    // let git_repo_path = "./src/git-repo".to_string();
    // let git_repo_name = "mustafa.git".to_string();
    //
    // let _full_repo_path = format!("{}/{}", git_repo_path, git_repo_name);

    let client = Client::new();

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let app = Router::new()
        .route("/:repo/git-upload-pack", post(upload_pack_rpc))
        .route("/:repo/git-receive-pack", post(recieve_pack_rpc))
        .route("/:repo/info/refs", get(get_info_refs))
        .route("/:repo/HEAD", get(get_info_refs))
        .layer(DefaultBodyLimit::disable())
        .fallback(fallback)
        .with_state(client);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
