#![allow(dead_code, unused_imports)]

use std::{
    collections::HashMap,
    env,
    ffi::OsStr,
    fs::File,
    io::Read,
    path::Path as StdPath,
    process::{exit, Output, Stdio},
};

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
use bytes::{Buf, BytesMut};
use nixpacks::{
    create_docker_image,
    nixpacks::{builder::docker::DockerBuilderOptions, plan::generator::GeneratePlanOptions},
};
use tokio::{
    io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    process::Command,
    time::sleep,
};

use hyper::{
    http::response::Builder as ResponseBuilder,
    service::{make_service_fn, service_fn},
    StatusCode,
};
use hyper::{Body, Request, Server};
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio_util::codec::{BytesCodec, FramedRead};

use axum::{
    body::Bytes,
    extract::{BodyStream, DefaultBodyLimit, Path, Query},
    http::header::HeaderMap,
    response::{Html, Response},
    routing::{get, post},
    Router,
};
use flate2::read::GzDecoder;
use futures_util::StreamExt;
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

async fn handler_auth(AuthBasic((id, password)): AuthBasic) -> String {
    if let Some(password) = password {
        format!("User '{}' with password '{}'", id, password)
    } else {
        format!("User '{}' without password", id)
    }
}

fn get_git_service(service: &str) -> &str {
    if service.starts_with("git-") {
        &service[4..]
    } else {
        ""
    }
}

#[derive(Deserialize, Debug)]
struct GitQuery {
    service: String,
}

async fn git_command<P, IA, S, IE, K, V>(dir: P, args: IA, envs: IE) -> Result<Output>
where
    P: AsRef<StdPath>,
    IA: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    IE: IntoIterator<Item = (K, V)>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .envs(envs)
        .output()
        .await?;

    Ok(output)
}

async fn get_info_refs(
    Path(repo): Path<String>,
    q: Query<GitQuery>,
    headers: HeaderMap,
) -> Response<Body> {
    let service = get_git_service(&q.service);
    if service != "receive-pack" && service != "upload-pack" {
        // TODO: change to update server into and return file
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    }

    let env = match headers.get("Git-Protocol").and_then(|v| v.to_str().ok()) {
        Some("version=2") => ("GIT_PROTOCOL".to_string(), "version=2".to_string()),
        _ => ("".to_string(), "".to_string()),
    };

    println!("env -> {:#?}", env);

    let envs = std::env::vars()
        .into_iter()
        .chain([env])
        .collect::<Vec<_>>();

    println!("repo -> {:#?}", repo);
    println!("q -> {:#?}", q);
    println!("headers -> {:#?}", headers);

    let full_repo_path = format!("{}/{}", "./src/git-repo", repo);

    let out = match git_command(
        &full_repo_path,
        &[service, "--stateless-rpc", "--advertise-refs", "."],
        envs,
    )
    .await
    {
        Ok(out) => out,
        Err(e) => {
            println!("error -> {:#?}", e);
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        }
    };

    let body = packet_write(&format!("# service={}\n", q.service));
    let body = [body, packet_flush(), out.stdout].concat();

    Response::builder()
        .no_cache()
        .header(
            "Content-Type",
            format!("application/x-git-{service}-advertisement"),
        )
        .body(Body::from(body))
        .unwrap()
}

pub async fn service_rpc(rpc: &str, repo: &str, headers: HeaderMap, body: Bytes) -> Response<Body> {
    println!("repo -> {:#?}", repo);
    println!("rpc -> {:#?}", rpc);
    println!("headers -> {:#?}", headers);

    let wd = env::current_dir().unwrap();

    let full_repo_path = format!("{}/{}/{}", wd.to_str().unwrap(), "src/git-repo", repo);
    println!("full_repo_path -> {:#?}", full_repo_path);

    let mut response = Response::builder()
        .header("Content-Type", format!("application/x-git-{rpc}-result"))
        .body(Body::empty())
        .unwrap();

    // TODO handler gzip

    // if headers.get("Content-Encoding").and_then(|enc| enc.to_str().ok()) == Some("gzip") {
    //     let mut reader = GzDecoder::new(body_bytes.as_ref());
    //     let new_bytes = match reader.read_to_end(&mut body_bytes) {
    //         Ok(_) => body_bytes,
    //         Err(_) => {
    //             *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    //             return response;
    //         }
    //     };
    // }

    let env = match headers.get("Git-Protocol").and_then(|v| v.to_str().ok()) {
        Some("version=2") => ("GIT_PROTOCOL".to_string(), "version=2".to_string()),
        _ => ("".to_string(), "".to_string()),
    };

    println!("env -> {:#?}", env);

    let envs = std::env::vars()
        .into_iter()
        .chain([env])
        .collect::<Vec<_>>();

    let mut cmd = Command::new("git");
    cmd.args(&[rpc, "--stateless-rpc", full_repo_path.as_str()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .envs(envs);

    let mut child = cmd.spawn().expect("failed to spawn command");

    let mut stdin = child.stdin.take().expect("failed to get stdin");

    if let Err(e) = stdin.write_all(&body).await {
        eprintln!("Failed to write to stdin: {}", e);
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        return response;
    }
    drop(stdin);

    let output = child
        .wait_with_output()
        .await
        .expect("Failed to read stdout/stderr");

    if !output.status.success() {
        eprintln!("Command failed: {:?}", output.status);
        eprintln!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    } else {
        println!("Command succeeded!");
        println!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
        println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
        *response.body_mut() = Body::from(output.stdout);
    }

    response
}

pub async fn recieve_pack_rpc(
    Path(repo): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    service_rpc("receive-pack", &repo, headers, body).await
}

pub async fn upload_pack_rpc(
    Path(repo): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    service_rpc("upload-pack", &repo, headers, body).await
}

trait GitServer {
    fn no_cache(self) -> Self;
    fn cache_forever(self) -> Self;
}

impl GitServer for ResponseBuilder {
    fn no_cache(self) -> Self {
        self.header("Expires", "Fri, 01 Jan 1980 00:00:00 GMT")
            .header("Pragma", "no-cache")
            .header("Cache-Control", "no-cache, max-age=0, must-revalidate")
    }
    fn cache_forever(self) -> Self {
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
    let _container_name = "go-example".to_string();
    let _image_name = "go-example:latest".to_string();
    let _container_src = "./src/go-example".to_string();
    let _network_name = "go-example-network".to_string();

    let git_repo_path = "./src/git-repo".to_string();
    let git_repo_name = "mustafa.git".to_string();

    let _full_repo_path = format!("{}/{}", git_repo_path, git_repo_name);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let app = Router::new()
        .route("/:repo/info/refs", get(get_info_refs))
        .route("/:repo/git-receive-pack", post(recieve_pack_rpc))
        .route("/:repo/git-upload-pack", post(upload_pack_rpc))
        .layer(DefaultBodyLimit::disable())
        .fallback(fallback);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

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
