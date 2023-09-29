use std::{
    ffi::OsStr,
    fs::File,
    io::Read,
    path::Path as StdPath,
    process::{Output, Stdio},
};

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    response::Response,
    routing::{delete, get, post},
    Router,
};
use hyper::{body::Bytes, http::response::Builder as ResponseBuilder, Body, HeaderMap, StatusCode};

use anyhow::Result;
use serde::Deserialize;
use serde_json::json;
use tokio::{io::AsyncWriteExt, process::Command};
use tower_http::limit::RequestBodyLimitLayer;

use crate::{configuration::Settings, docker::build_docker, startup::AppState};

pub fn router(state: AppState, config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route("/:repo/git-upload-pack", post(upload_pack_rpc))
        .route("/:repo/git-receive-pack", post(recieve_pack_rpc))
        .route("/:repo/info/refs", get(get_info_refs))
        .route(
            "/:repo/HEAD",
            get(|Path(repo): Path<String>, State(AppState { base, .. }): State<AppState>| async move { get_file_text(&base, &repo, "HEAD").await }),
        )
        .route(
            "/:repo/objects/info/alternates",
            get(|Path(repo): Path<String>, State(AppState { base, .. }): State<AppState>| async move {
                get_file_text(&base, &repo, "objects/info/alternates").await
            }),
        )
        .route(
            "/:repo/objects/info/http-alternates",
            get(|Path(repo): Path<String>, State(AppState { base, .. }): State<AppState>| async move {
                get_file_text(&base, &repo, "objects/info/http-alternates").await
            }),
        )
        .route("/:repo/objects/info/packs", get(get_info_packs))
        .route(
            "/:repo/objects/info/:file",
            get(
                |Path((repo, head, file)): Path<(String, String, String)>,State(AppState { base, .. }): State<AppState>| async move {
                    get_file_text(&base, &repo, format!("{}/{}", head, file).as_ref()).await
                },
            ),
        )
        .route("/:repo/objects/:head/:hash", get(get_loose_object))
        .route("/:repo/objects/packs/:file", get(get_pack_or_idx_file))

        // not git server related
        .route("/:repo", post(create_new_repo))
        .route("/:repo", delete(delete_repo))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(config.body_limit()))
        .with_state(state)
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

fn get_git_service(service: &str) -> &str {
    match service.starts_with("git-") {
        true => &service[4..],
        false => "",
    }
}

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

pub async fn get_info_packs(
    Path(repo): Path<String>,
    State(AppState { base, .. }): State<AppState>,
) -> Response<Body> {
    let path = match repo.ends_with(".git") {
        true => format!("{base}/{repo}/objects/info/packs"),
        false => format!("{base}/{repo}.git/objects/info/packs"),
    };

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return Response::builder().status(404).body(Body::empty()).unwrap(),
    };

    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    Response::builder()
        .no_cache()
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Body::from(contents))
        .unwrap()
}

pub async fn get_loose_object(
    Path((repo, head, hash)): Path<(String, String, String)>,
    State(AppState { base, .. }): State<AppState>,
) -> Response<Body> {
    let path = match repo.ends_with(".git") {
        true => format!("{base}/{repo}/objects/{head}/{hash}"),
        false => format!("{base}/{repo}.git/objects/{head}{hash}"),
    };
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return Response::builder().status(404).body(Body::empty()).unwrap(),
    };

    let mut contents = Vec::new();
    file.read_to_end(&mut contents).unwrap();

    Response::builder()
        .cache_forever()
        .header("Content-Type", "application/x-git-loose-object")
        .body(Body::from(contents))
        .unwrap()
}

pub async fn get_pack_or_idx_file(
    Path((repo, file)): Path<(String, String)>,
    State(AppState { base, .. }): State<AppState>,
) -> Response<Body> {
    let path = match repo.ends_with(".git") {
        true => format!("{base}/{repo}/objects/pack/{file}"),
        false => format!("{base}/{repo}.git/objects/pack{file}"),
    };
    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(_) => return Response::builder().status(404).body(Body::empty()).unwrap(),
    };

    let res = Response::builder().cache_forever();

    let res = match StdPath::new(&path).extension().and_then(|ext| ext.to_str()) {
        Some("pack") => res.header("Content-Type", "application/x-git-packed-objects"),
        Some("idx") => res.header("Content-Type", "application/x-git-packed-objects-toc"),
        _ => return Response::builder().status(404).body(Body::empty()).unwrap(),
    };

    let mut contents = Vec::new();
    file.read_to_end(&mut contents).unwrap();

    res.body(Body::from(contents)).unwrap()
}

pub async fn get_file_text(base: &str, repo: &str, file: &str) -> Response<Body> {
    let path = match repo.ends_with(".git") {
        true => format!("{base}/{repo}/{file}"),
        false => format!("{base}/{repo}.git/{file}"),
    };

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return Response::builder().status(404).body(Body::empty()).unwrap(),
    };

    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    Response::builder()
        .no_cache()
        .header("Content-Type", "text/plain")
        .body(Body::from(contents))
        .unwrap()
}

pub async fn recieve_pack_rpc(
    Path(repo): Path<String>,
    State(AppState { base, .. }): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    let path = match repo.ends_with(".git") {
        true => format!("{base}/{repo}"),
        false => format!("{base}/{repo}.git"),
    };
    let res = service_rpc("receive-pack", &path, headers, body).await;
    res

    // let container_name = "go-example".to_string();
    // let repo_src = "./src/git-repo/mustafa.git".to_string();
    // let container_src = "./src/git-repo/mustafa.git/master".to_string();
    //
    // if let Err(_e) = git2::Repository::clone(&repo_src, &container_src) {
    //     // try to pull
    //     if let Err(e) = git2::Repository::open(&container_src).and_then(|repo| {
    //         repo.find_remote("origin")
    //             .and_then(|mut remote| remote.fetch(&["master"], None, None))
    //     }) {
    //         // try to delete the folder and clone again
    //         println!("error -> {:#?}", e);
    //         std::fs::remove_dir_all(&container_src).unwrap();
    //
    //         if let Err(e) = git2::Repository::clone(&repo_src, &container_src) {
    //             // if this doesnt work then something is wrong
    //             println!("error -> {:#?}", e);
    //             return Response::builder()
    //                 .status(StatusCode::INTERNAL_SERVER_ERROR)
    //                 .body(Body::empty())
    //                 .unwrap();
    //         };
    //     };
    // };
    //
    // if let Err(e) = build_docker(&container_name, &container_src).await {
    //     println!("error -> {:#?}", e);
    //     return Response::builder()
    //         .status(StatusCode::INTERNAL_SERVER_ERROR)
    //         .body(Body::empty())
    //         .unwrap();
    // };
    //
    // println!("container run on go-example:localhost:3000");
    //
    // res
}

pub async fn upload_pack_rpc(
    Path(repo): Path<String>,
    State(AppState { base, .. }): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    let path = match repo.ends_with(".git") {
        true => format!("{base}/{repo}"),
        false => format!("{base}/{repo}.git"),
    };

    service_rpc("upload-pack", &path, headers, body).await
}

pub async fn service_rpc(rpc: &str, path: &str, headers: HeaderMap, body: Bytes) -> Response<Body> {
    let mut response = Response::builder()
        .header("Content-Type", format!("application/x-git-{rpc}-result"))
        .body(Body::empty())
        .unwrap();

    let body = match headers
        .get("Content-Encoding")
        .and_then(|enc| enc.to_str().ok())
    {
        Some("gzip") => {
            let mut reader = flate2::read::GzDecoder::new(body.as_ref());
            let mut new_bytes = Vec::new();
            match reader.read_to_end(&mut new_bytes) {
                Ok(_) => Bytes::from(new_bytes),
                Err(_) => {
                    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                    return response;
                }
            }
        }
        _ => body,
    };

    let env = match headers.get("Git-Protocol").and_then(|v| v.to_str().ok()) {
        Some("version=2") => ("GIT_PROTOCOL".to_string(), "version=2".to_string()),
        _ => ("".to_string(), "".to_string()),
    };

    let envs = std::env::vars()
        .into_iter()
        .chain([env])
        .collect::<Vec<_>>();

    let mut cmd = Command::new("git");
    cmd.args(&[rpc, "--stateless-rpc", path])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .envs(envs);

    let mut child = cmd.spawn().expect("failed to spawn command");
    let mut stdin = child.stdin.take().expect("failed to get stdin");

    if let Err(e) = stdin.write_all(&body).await {
        tracing::error!("Failed to write to stdin: {}", e);
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        return response;
    }
    drop(stdin);

    let output = child
        .wait_with_output()
        .await
        .expect("Failed to read stdout/stderr");

    if !output.status.success() {
        tracing::error!("Command failed: {:?}", output.status);
        tracing::error!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    } else {
        tracing::info!("Command succeeded!");
        tracing::info!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
        tracing::info!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
        *response.body_mut() = Body::from(output.stdout);
    }

    response
}

#[derive(Deserialize, Debug)]
pub struct GitQuery {
    service: String,
}

pub async fn get_info_refs(
    Path(repo): Path<String>,
    State(AppState { base, .. }): State<AppState>,
    Query(GitQuery { service }): Query<GitQuery>,
    headers: HeaderMap,
) -> Response<Body> {
    let service = get_git_service(&service);

    let path = match repo.ends_with(".git") {
        true => format!("{base}/{repo}"),
        false => format!("{base}/{repo}.git"),
    };
    if service != "receive-pack" && service != "upload-pack" {
        git_command(
            &path,
            &["update-server-info"],
            std::iter::empty::<(String, String)>(),
        )
        .await
        .unwrap();

        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(_) => return Response::builder().status(404).body(Body::empty()).unwrap(),
        };

        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();

        return Response::builder()
            .no_cache()
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(Body::from(contents))
            .unwrap();
    }

    let env = match headers.get("Git-Protocol").and_then(|v| v.to_str().ok()) {
        Some("version=2") => ("GIT_PROTOCOL".to_string(), "version=2".to_string()),
        _ => ("".to_string(), "".to_string()),
    };

    let envs = std::env::vars()
        .into_iter()
        .chain([env])
        .collect::<Vec<_>>();

    let out = match git_command(
        &path,
        &[service, "--stateless-rpc", "--advertise-refs", "."],
        envs,
    )
    .await
    {
        Ok(out) => out,
        Err(e) => {
            tracing::error!("Failed to run git command: {}", e);
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        }
    };

    let body = packet_write(&format!("# service=git-{}\n", service));
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

pub async fn create_new_repo(
    Path(repo): Path<String>,
    State(AppState { base, .. }): State<AppState>,
) -> Response<Body> {
    // check if repo exists
    let path = match repo.ends_with(".git") {
        true => format!("{base}/{repo}"),
        false => format!("{base}/{repo}.git"),
    };

    match File::open(&path) {
        Ok(_) => {
            return Response::builder()
                .status(StatusCode::CONFLICT)
                .body(Body::from(json!({"message": "repo exist"}).to_string()))
                .unwrap()
        }
        Err(_) => {}
    };

    match git2::Repository::init_bare(&path) {
        Ok(_) => Response::builder()
            .body(Body::from(
                json!({"message": "repo created successfully"}).to_string(),
            ))
            .unwrap(),
        Err(e) => {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(
                    json!({"message": format!("failed to init repo: {}", e)}).to_string(),
                ))
                .unwrap()
        }
    }
}

pub async fn delete_repo(
    Path(repo): Path<String>,
    State(AppState { base, .. }): State<AppState>,
) -> Response<Body> {
    let path = match repo.ends_with(".git") {
        true => format!("{base}/{repo}"),
        false => format!("{base}/{repo}.git"),
    };

    // check if repo exists
    match File::open(&path) {
        Err(_) => {
            return Response::builder()
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .body(Body::from(
                    json!({"message": "repo doesn't exist"}).to_string(),
                ))
                .unwrap()
        }

        Ok(_) => {}
    };

    match std::fs::remove_dir_all(&path) {
        Ok(_) => Response::builder()
            .body(Body::from(
                json!({"message": "repo deleted successfully"}).to_string(),
            ))
            .unwrap(),
        Err(e) => {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(
                    json!({"message": format!("failed to delete repo: {}", e)}).to_string(),
                ))
                .unwrap()
        }
    }
}
