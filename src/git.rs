use std::{
    env,
    ffi::OsStr,
    fs::File,
    io::Read,
    path::Path as StdPath,
    process::{Output, Stdio},
};

use axum::{
    extract::{Path, Query},
    response::Response,
};
use hyper::{body::Bytes, http::response::Builder as ResponseBuilder, Body, HeaderMap, StatusCode};

use anyhow::Result;
use serde::Deserialize;
use tokio::{io::AsyncWriteExt, process::Command};

use crate::docker::build_docker;

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
    if service.starts_with("git-") {
        &service[4..]
    } else {
        ""
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

// async fn handler_auth(AuthBasic((id, password)): AuthBasic) -> String {
//     if let Some(password) = password {
//         format!("User '{}' with password '{}'", id, password)
//     } else {
//         format!("User '{}' without password", id)
//     }
// }

// pub async fn get_file_text(path: &str) -> impl Fn(&str) -> Response<Body> {
//     move |path: &str| {
//         let mut file = File::open(path).unwrap();
//         let mut contents = String::new();
//         file.read_to_string(&mut contents).unwrap();
//         Response::builder()
//             .cache_forever()
//             .body(Body::from(contents))
//             .unwrap()
//     }
// }

pub async fn recieve_pack_rpc(
    Path(repo): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    let mut res = service_rpc("receive-pack", &repo, headers, body).await;
    let container_name = "go-example".to_string();
    let repo_src = "./src/git-repo/mustafa.git".to_string();
    let container_src = "./src/git-repo/mustafa.git/master".to_string();

    if let Err(e) = git2::Repository::clone(&repo_src, &container_src) {
        // try to pull
        if let Err(e) = git2::Repository::open(&container_src).and_then(|repo| {
            repo.find_remote("origin")
                .and_then(|mut remote| remote.fetch(&["master"], None, None))
        }) {
            // try to delete the folder and clone again
            std::fs::remove_dir_all(&container_src).unwrap();

            if let Err(e) = git2::Repository::clone(&repo_src, &container_src) {
                // if this doesnt work then something is wrong
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap();
            };
        };
    };

    if let Err(e) = build_docker(&container_name, &container_src).await {
        println!("error -> {:#?}", e);
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap();
    };

    *res.body_mut() = Body::from("container run on go-example:localhost:3000");
    res
}

pub async fn upload_pack_rpc(
    Path(repo): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    service_rpc("upload-pack", &repo, headers, body).await
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

#[derive(Deserialize, Debug)]
pub struct GitQuery {
    service: String,
}

pub async fn get_info_refs(
    Path(repo): Path<String>,
    Query(GitQuery { service }): Query<GitQuery>,
    headers: HeaderMap,
) -> Response<Body> {
    let service = get_git_service(&service);
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
