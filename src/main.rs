// #![allow(dead_code, unused_imports)]

use anyhow::Result;

use pemasak_infra::git::{get_info_refs, recieve_pack_rpc, upload_pack_rpc};

use std::net::SocketAddr;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};

pub async fn fallback(uri: axum::http::Uri) -> impl axum::response::IntoResponse {
    println!("route not found uri -> {:#?}", uri);
    (
        axum::http::StatusCode::NOT_FOUND,
        format!("No route {}", uri),
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    let _container_name = "go-example".to_string();
    let _image_name = "go-example:latest".to_string();
    let _container_src = "./src/go-example".to_string();
    let _network_name = "go-example-network".to_string();

    let git_repo_path = "./src/git-repo".to_string();
    let git_repo_name = "mustafa.git".to_string();

    let _full_repo_path = format!("{}/{}", git_repo_path, git_repo_name);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let app = Router::new()
        .route("/:repo/git-upload-pack", post(upload_pack_rpc))
        .route("/:repo/git-receive-pack", post(recieve_pack_rpc))
        .route("/:repo/info/refs", get(get_info_refs))
        .route("/:repo/HEAD", get(get_info_refs))
        .layer(DefaultBodyLimit::disable())
        .fallback(fallback);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
