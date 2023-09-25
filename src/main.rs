#![allow(dead_code, unused_imports)]  

use std::{collections::HashMap, env, process::Stdio};

use anyhow::Result;
use axum_auth::AuthBasic;
use bollard::{
    container::{
        Config, CreateContainerOptions, InspectContainerOptions, ListContainersOptions,
        StartContainerOptions, StopContainerOptions,
    },
    image::ListImagesOptions,
    network::{ConnectNetworkOptions, InspectNetworkOptions, ListNetworksOptions},
    Docker,
};
use nixpacks::{
    create_docker_image,
    nixpacks::{builder::docker::DockerBuilderOptions, plan::generator::GeneratePlanOptions},
};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::sleep,
};

use hyper::{
    http::response::Builder as ResponseBuilder,
    service::{make_service_fn, service_fn},
};
use hyper::{Body, Request, Server};
use std::convert::Infallible;
use std::net::SocketAddr;

use axum::{
    extract::{Path, Query},
    http::header::HeaderMap,
    response::{Html, Response},
    routing::{get, post},
    Router,
};
use flate2::read::GzDecoder;
use serde::Deserialize;

fn packet_write(s: &str) -> Vec<u8> {
    let length = s.len() + 4;
    let mut length_hex = format!("{:x}", length);

    while length_hex.len() % 4 != 0 {
        length_hex.insert(0, '0');
    }

    let result = format!("{}{}", length_hex, s);

    result.into_bytes()
}

fn packet_flush() -> Vec<u8> {
    "0000".into()
}

pub async fn fallback(uri: axum::http::Uri) -> impl axum::response::IntoResponse {
    println!("route not found uri -> {:#?}", uri);
    (
        axum::http::StatusCode::NOT_FOUND,
        format!("No route {}", uri),
    )
}

async fn handlerAuth(AuthBasic((id, password)): AuthBasic) -> String {
    if let Some(password) = password {
        format!("User '{}' with password '{}'", id, password)
    } else {
        format!("User '{}' without password", id)
    }
}

#[derive(Deserialize, Debug)]
struct GitQuery {
    service: String,
}

async fn get_info_handler(
    Path(repo): Path<String>,
    q: Query<GitQuery>,
    headers: HeaderMap,
) -> Response<Body> {
    let version = headers
        .get("Git-Protocol")
        .map(|v| i32::from_str_radix(v.to_str().unwrap_or_default(), 10).unwrap_or_default())
        .unwrap_or(0);

    println!("repo -> {:#?}", repo);
    println!("q -> {:#?}", q);
    println!("headers -> {:#?}", headers);
    println!("version -> {:#?}", version);

    let full_repo_path = format!("{}/{}", "./src/git-repo", repo);

    let out = Command::new("git-receive-pack")
        .env("GIT_PROTOCOL", "2")
        .args(&[
            "--stateless-rpc",
            "--advertise-refs",
            full_repo_path.as_str(),
        ])
        .output()
        .await
        .unwrap();

    let out_str = String::from_utf8_lossy(&out.stdout).to_string();

    println!("cmd -> {:#?}", out_str);
    let body = packet_write(&format!("# service={}\n", q.service));
    let body = [body, out.stdout, packet_flush()].concat();
    println!("body -> {:#?}", String::from_utf8_lossy(&body));

    // Response::new(Body::from(out.stdout))
    Response::builder()
        .hdr_no_cache()
        .header(
            "Content-Type",
            "application/x-git-receive-pack-advertisement",
        )
        .body(Body::from(body))
        .unwrap()
}

pub async fn service_rpc(
    Path(repo): Path<String>,
    headers: HeaderMap,
    mut req: Request<Body>,
) -> Response<Body> {
    let rpc = "git-receive-pack";
    // if let Some(default_env) = DEFAULT_CONFIG.default_env {
    //     env.push(default_env);
    // }

    println!("repo -> {:#?}", repo);
    println!("rpc -> {:#?}", rpc);
    println!("headers -> {:#?}", headers);

    let full_repo_path = format!("{}/{}", "./src/git-repo", repo);
    let mut cmd = Command::new(rpc);
    cmd.args(&[
        "--stateless-rpc",
        "--advertise-refs",
        full_repo_path.as_str(),
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped());

    let mut child = cmd.spawn().expect("failed to spawn command");
    let mut stdin = child.stdin.take().expect("failed to get stdin");
    let mut stdout = child.stdout.take().expect("failed to get stdout");

    // Write request body to stdin
    let body_bytes = hyper::body::to_bytes(req.body_mut()).await.unwrap();

    // let mut reader = match req
    //     .headers()
    //     .get("Content-Encoding")
    //     .and_then(|enc| enc.to_str().ok())
    // {
    //     Some("gzip") => Box::new(GzDecoder::new(body_bytes)),
    //     _ => Box::new(body_bytes),
    // };

    // println!("body_bytes -> {:#?}", String::from_utf8_lossy(&body_bytes));

    stdin.write_all(&body_bytes).await.expect("failed to write to stdin");
    let mut out_body = String::new();
    stdout.read_to_string(&mut out_body).await.unwrap();
    println!("out_body -> {:#?}", out_body);

    Response::builder()
        .header("Content-Type", "application/x-git-receive-pack-result")
        .header("Connection", "Keep-Alive")
        .header("Transfer-Encoding", "chunked")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(out_body.as_bytes().to_owned()))
        // .body(Body::empty())
        .unwrap()
}

trait GitServer {
    fn hdr_no_cache(self) -> Self;
    fn hdr_cache_forever(self) -> Self;
}

impl GitServer for ResponseBuilder {
    fn hdr_no_cache(self) -> Self {
        self.header("Expires", "Fri, 01 Jan 1980 00:00:00 GMT")
            .header("Pragma", "no-cache")
            .header("Cache-Control", "no-cache, max-age=0, must-revalidate")
    }
    fn hdr_cache_forever(self) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();

        let expire = now + 31536000;
        self.header("Date", now.to_string().as_str())
            .header("Expires", expire.to_string().as_str())
            .header("Cache-Control", "public, max-age=31536000")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let container_name = "go-example".to_string();
    let image_name = "go-example:latest".to_string();
    let container_src = "./src/go-example".to_string();
    let network_name = "go-example-network".to_string();

    let git_repo_path = "./src/repo".to_string();

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let app = Router::new()
        .fallback(fallback)
        .route("/:repo/info/refs", get(get_info_handler))
        .fallback(fallback)
        .route("/:repo/git-receive-pack", post(service_rpc))
        // .fallback(fallback)
    ;
    // .route("/:repo/*path", get(handlerGeneric));

    // /mustafa.git/info/refs?service=git-receive-pack

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    // let make_service = make_service_fn(|_conn| async {
    //     Ok::<_, Infallible>(service_fn(|req: Request<Body>| async move {
    //         println!("uri -> {:#}", req.uri());
    //
    //         let mut cmd = Command::new("git-http-backend");
    //         cmd.env("GIT_PROJECT_ROOT", "./src/git-repo/")
    //             .env("GIT_HTTP_EXPORT_ALL", "");
    //
    //         // Extract path info from the URI
    //         if let Some(path_info) = req.uri().authority() {
    //             println!("path_info -> {:#?}", path_info);
    //             cmd.env("PATH_INFO", path_info.as_str());
    //         }
    //
    //         // Extract method from the Request
    //         cmd.env("REQUEST_METHOD", req.method().as_str());
    //
    //         // Extract user information if authenticated
    //         // This is just an example, and you may have your own logic for user authentication.
    //         println!("req -> {:#?}", req);
    //         let url_str = format!(
    //             "{}://{}{}",
    //             req.uri()
    //                 .scheme()
    //                 .map(|s| s.to_string())
    //                 .unwrap_or("http".to_string()),
    //             req.uri().authority().unwrap(),
    //             req.uri().path_and_query().unwrap()
    //         );
    //         if let Ok(auth) = url::Url::parse(&url_str) {
    //             println!("auth -> {:#?}", auth);
    //             if auth.username() != "" {
    //                 cmd.env("REMOTE_USER", auth.username());
    //             }
    //         } else {
    //             panic!("Invalid URL");
    //         }
    //
    //         let (body, err) = do_cgi(req, cmd).await;
    //         println!("err -> {:?}", String::from_utf8_lossy(&err));
    //         Ok::<Response<Body>, Infallible>(body)
    //     }))
    //     // Ok::<_, Infallible>(service_fn(handle))
    // });
    //
    // let server = Server::bind(&addr).serve(make_service);
    // if let Err(e) = server.await {
    //     eprintln!("server error: {}", e);
    // }

    // let plan_options = GeneratePlanOptions::default();
    // let build_options = DockerBuilderOptions {
    //     name: Some(container_name.clone()),
    //     quiet: false,
    //     ..Default::default()
    // };
    // let envs = vec![];
    // create_docker_image(&container_src, envs, &plan_options, &build_options).await?;
    //
    // let docker = Docker::connect_with_local_defaults()?;
    //
    // let images = &docker
    //     .list_images(Some(ListImagesOptions::<String> {
    //         all: false,
    //         filters: HashMap::from([("reference".to_string(), vec![image_name.clone()])]),
    //         ..Default::default()
    //     }))
    //     .await
    //     .unwrap();
    //
    // for image in images {
    //     println!("images -> {:#?}", image);
    // }
    //
    // let _image = images.first().ok_or(anyhow::anyhow!("No image found"))?;
    //
    // // remove container if it exists
    // let containers = docker
    //     .list_containers(Some(ListContainersOptions::<String> {
    //         all: true,
    //         filters: HashMap::from([("name".to_string(), vec![container_name.clone()])]),
    //         ..Default::default()
    //     }))
    //     .await?
    //     .into_iter()
    //     .collect::<Vec<_>>();
    //
    // for container in &containers {
    //     println!("container -> {:?}", container.names);
    // }
    //
    // if !containers.is_empty() {
    //     docker
    //         .stop_container(&container_name, None::<StopContainerOptions>)
    //         .await?;
    //
    //     docker
    //         .remove_container(
    //             containers.first().unwrap().id.as_ref().unwrap(),
    //             None::<bollard::container::RemoveContainerOptions>,
    //         )
    //         .await?;
    // }
    //
    // let config = Config {
    //     image: Some(image_name.clone()),
    //     ..Default::default()
    // };
    //
    // let res = &docker
    //     .create_container(
    //         Some(CreateContainerOptions {
    //             name: container_name.clone(),
    //             platform: None,
    //         }),
    //         config,
    //     )
    //     .await?;
    //
    // println!("create response-> {:#?}", res);
    //
    // // check if network exists
    // let network = docker
    //     .list_networks(Some(ListNetworksOptions {
    //         filters: HashMap::from([("name".to_string(), vec![network_name.clone()])]),
    //     }))
    //     .await?
    //     .first()
    //     .map(|n| n.to_owned());
    //
    // let network = match network {
    //     Some(n) => {
    //         println!("Existing network id -> {:?}", n.id);
    //         n
    //     }
    //     None => {
    //         let options = bollard::network::CreateNetworkOptions {
    //             name: network_name.clone(),
    //             ..Default::default()
    //         };
    //         let res = docker.create_network(options).await?;
    //         println!("create network response-> {:#?}", res);
    //
    //         docker
    //             .list_networks(Some(ListNetworksOptions {
    //                 filters: HashMap::from([("name".to_string(), vec![network_name.clone()])]),
    //             }))
    //             .await?
    //             .first()
    //             .map(|n| n.to_owned())
    //             .ok_or(anyhow::anyhow!("No network found after make one???"))?
    //     }
    // };
    //
    // // connect container to network
    // docker
    //     .connect_network(
    //         &network_name,
    //         ConnectNetworkOptions {
    //             container: container_name.clone(),
    //             ..Default::default()
    //         },
    //     )
    //     .await?;
    //
    // println!("connect network response-> {:#?}", res);
    //
    // docker
    //     .start_container(&container_name, None::<StartContainerOptions<String>>)
    //     .await?;
    //
    // sleep(std::time::Duration::from_secs(1)).await;
    //
    // //inspect network
    // let network_inspect = docker
    //     .inspect_network(
    //         &network.id.unwrap(),
    //         Some(InspectNetworkOptions::<&str> {
    //             verbose: true,
    //             ..Default::default()
    //         }),
    //     )
    //     .await?;
    //
    // println!(
    //     "ipv4 address -> {:#?}",
    //     network_inspect
    //         .containers
    //         .unwrap()
    //         .get(&res.id)
    //         .unwrap()
    //         .ipv4_address
    // );

    Ok(())
}
