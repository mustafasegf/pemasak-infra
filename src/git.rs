use std::{
    ffi::OsStr,
    fs::File,
    io::Read,
    path::Path as StdPath,
    process::{Output, Stdio},
};

use argon2::{
    password_hash::{PasswordHash, PasswordVerifier},
    Argon2,
};
use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use git2::Repository;
use http_body::combinators::UnsyncBoxBody;
use hyper::{
    body::Bytes, http::response::Builder as ResponseBuilder, Body, HeaderMap, Request, StatusCode,
};

use anyhow::Result;
use serde::Deserialize;
use tokio::{io::AsyncWriteExt, process::Command};
use tower_http::limit::RequestBodyLimitLayer;

use crate::{configuration::Settings, startup::AppState, queue::BuildQueueItem};

use data_encoding::BASE64;

async fn basic_auth<B>(
    State(AppState { pool, git_auth, .. }): State<AppState>,
    headers: HeaderMap,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response<UnsyncBoxBody<Bytes, axum::Error>>, hyper::Response<Body>> {
    if !git_auth {
        return Ok(next.run(request).await);
    }

    let auth_err = Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("WWW-Authenticate", "Basic realm=\"git\"")
        .body(Body::empty())
        .unwrap();

    let auth_failed = Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("WWW-Authenticate", "Basic realm=\"failed to login\"")
        .body(Body::empty())
        .unwrap();

    match headers.get("Authorization").and_then(|v| v.to_str().ok()) {
        None => Err(auth_err),
        Some(auth) => {
            let mut parts = auth.split_whitespace();
            let scheme = parts.next().unwrap_or("");
            let token = parts.next().unwrap_or("");

            if scheme != "Basic" {
                return Err(auth_err);
            }

            let decoded = BASE64.decode(token.as_bytes()).unwrap();
            let decoded = String::from_utf8(decoded).unwrap();
            let mut parts = decoded.split(':');
            let owner_name = parts.next().unwrap_or("");
            let token = parts.next().unwrap_or("");

            let tokens = match sqlx::query!(
                r#"SELECT api_token.token
                    FROM project_owners
                    JOIN projects ON project_owners.id = projects.owner_id
                    JOIN api_token ON projects.id = api_token.project_id
                    WHERE project_owners.name = $1
                "#,
                owner_name
            )
            .fetch_all(&pool)
            .await
            {
                Ok(tokens) => tokens,
                Err(sqlx::Error::RowNotFound) => return Err(auth_failed),
                Err(_) => return Err(auth_err),
            };

            let hasher = Argon2::default();
            let authenticaed = tokens.iter().any(|rec| {
                PasswordHash::new(&rec.token)
                    .and_then(|hash| hasher.verify_password(token.as_bytes(), &hash))
                    .is_ok()
            });
            if !authenticaed {
                return Err(auth_failed);
            }

            Ok(next.run(request).await)
        }
    }
}

pub fn router(state: AppState, config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route("/:owner/:repo/git-upload-pack", post(upload_pack_rpc))
        .route("/:owner/:repo/git-receive-pack", post(recieve_pack_rpc))
        .route("/:owner/:repo/info/refs", get(get_info_refs))
        .route(
            "/:owner/:repo/HEAD",
            get(
                |Path((owner, repo)): Path<(String, String)>,
                 State(AppState { base, .. }): State<AppState>| async move {
                    get_file_text(&base, &owner, &repo, "HEAD").await
                },
            ),
        )
        .route(
            "/:owner/:repo/objects/info/alternates",
            get(
                |Path((owner, repo)): Path<(String, String)>,
                 State(AppState { base, .. }): State<AppState>| async move {
                    get_file_text(&base, &owner, &repo, "objects/info/alternates").await
                },
            ),
        )
        .route(
            "/:owner/:repo/objects/info/http-alternates",
            get(
                |Path((owner, repo)): Path<(String, String)>,
                 State(AppState { base, .. }): State<AppState>| async move {
                    get_file_text(&base, &owner, &repo, "objects/info/http-alternates").await
                },
            ),
        )
        .route("/:owner/:repo/objects/info/packs", get(get_info_packs))
        .route(
            "/:owner/:repo/objects/info/:file",
            get(
                |Path((owner, repo, head, file)): Path<(String, String, String, String)>,
                 State(AppState { base, .. }): State<AppState>| async move {
                    get_file_text(&base, &owner, &repo, format!("{}/{}", head, file).as_ref()).await
                },
            ),
        )
        .route("/:owner/:repo/objects/:head/:hash", get(get_loose_object))
        .route(
            "/:owner/:repo/objects/packs/:file",
            get(get_pack_or_idx_file),
        )
        .route_layer(middleware::from_fn_with_state(state, basic_auth))
        // not git server related
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(config.body_limit()))
    // .with_state(state)
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

pub async fn get_file_text(base: &str, owner: &str, repo: &str, file: &str) -> Response<Body> {
    let path = match repo.ends_with(".git") {
        true => format!("{base}/{owner}/{repo}/{file}"),
        false => format!("{base}/{owner}/{repo}.git/{file}"),
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

fn fast_forward(
    repo: &Repository,
    lb: &mut git2::Reference,
    rc: &git2::AnnotatedCommit,
) -> Result<(), git2::Error> {
    let name = match lb.name() {
        Some(s) => s.to_string(),
        None => String::from_utf8_lossy(lb.name_bytes()).to_string(),
    };
    let msg = format!("Fast-Forward: Setting {} to id: {}", name, rc.id());
    println!("{}", msg);
    lb.set_target(rc.id(), &msg)?;
    repo.set_head(&name)?;
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::default()
            // For some reason the force is required to make the working directory actually get updated
            // I suspect we should be adding some logic to handle dirty working directory states
            // but this is just an example so maybe not.
            .force(),
    ))?;
    Ok(())
}

fn normal_merge(
    repo: &Repository,
    local: &git2::AnnotatedCommit,
    remote: &git2::AnnotatedCommit,
) -> Result<(), git2::Error> {
    let local_tree = repo.find_commit(local.id())?.tree()?;
    let remote_tree = repo.find_commit(remote.id())?.tree()?;
    let ancestor = repo
        .find_commit(repo.merge_base(local.id(), remote.id())?)?
        .tree()?;
    let mut idx = repo.merge_trees(&ancestor, &local_tree, &remote_tree, None)?;

    if idx.has_conflicts() {
        println!("Merge conflicts detected...");
        repo.checkout_index(Some(&mut idx), None)?;
        return Ok(());
    }
    let result_tree = repo.find_tree(idx.write_tree_to(repo)?)?;
    // now create the merge commit
    let msg = format!("Merge: {} into {}", remote.id(), local.id());
    let sig = repo.signature()?;
    let local_commit = repo.find_commit(local.id())?;
    let remote_commit = repo.find_commit(remote.id())?;
    // Do our merge commit and set current branch head to that commit.
    let _merge_commit = repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &msg,
        &result_tree,
        &[&local_commit, &remote_commit],
    )?;
    // Set working tree to match head.
    repo.checkout_head(None)?;
    Ok(())
}

pub async fn recieve_pack_rpc(
    Path((owner, repo)): Path<(String, String)>,
    State(AppState {
        base, build_channel, ..
    }): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    let path = match repo.ends_with(".git") {
        true => format!("{base}/{owner}/{repo}"),
        false => format!("{base}/{owner}/{repo}.git"),
    };
    let res = service_rpc("receive-pack", &path, headers, body).await;
    let container_src = format!("{path}/master");
    let container_name = format!("{owner}-{}", repo.trim_end_matches(".git")).replace('.', "-") ;

    // TODO: clean up this mess
    if let Err(_e) = git2::Repository::clone(&path, &container_src) {
        tracing::info!("repo already cloned");
        // try to pull
        let repo = git2::Repository::open(&container_src).unwrap();
        let mut fo = git2::FetchOptions::new();
        fo.download_tags(git2::AutotagOption::All);

        let mut remote = repo.find_remote("origin").unwrap();
        remote.fetch(&["master"], Some(&mut fo), None).unwrap();

        let fetch_head = repo.find_reference("FETCH_HEAD").unwrap();
        let fetch_commit = repo.reference_to_annotated_commit(&fetch_head).unwrap();

        let analysis = repo.merge_analysis(&[&fetch_commit]).unwrap();

        if analysis.0.is_fast_forward() {
            tracing::info!("fast forward");
            let refname = "refs/heads/master";
            match repo.find_reference(refname) {
                Ok(mut r) => {
                    fast_forward(&repo, &mut r, &fetch_commit).unwrap();
                }
                Err(_) => {
                    // The branch doesn't exist so just set the reference to the
                    // commit directly. Usually this is because you are pulling
                    // into an empty repository.
                    repo.reference(
                        refname,
                        fetch_commit.id(),
                        true,
                        &format!("Setting {} to master", fetch_commit.id()),
                    )
                    .unwrap();
                    repo.set_head(refname).unwrap();
                    repo.checkout_head(Some(
                        git2::build::CheckoutBuilder::default()
                            .allow_conflicts(true)
                            .conflict_style_merge(true)
                            .force(),
                    ))
                    .unwrap();
                }
            };
        } else {
            tracing::info!("merge");
            let head_commit = repo
                .reference_to_annotated_commit(&repo.head().unwrap())
                .unwrap();
            normal_merge(&repo, &head_commit, &fetch_commit).unwrap();
        };

        if false {
            // try to delete the folder and clone again
            // tracing::error!("can't fetch repo -> {:#?}", e);
            std::fs::remove_dir_all(&container_src).unwrap();

            if let Err(e) = git2::Repository::clone(&path, &container_src) {
                // if this doesnt work then something is wrong
                println!("error -> {:#?}", e);
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap();
            };
        };
    };

    tokio::spawn(async move {
        build_channel.send(
            BuildQueueItem {
                container_name,
                container_src,
                owner,
                repo,
            }
        ).await
    });
    
    res
}

pub async fn upload_pack_rpc(
    Path((owner, repo)): Path<(String, String)>,
    State(AppState { base, .. }): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    let path = match repo.ends_with(".git") {
        true => format!("{base}/{owner}/{repo}"),
        false => format!("{base}/{owner}/{repo}.git"),
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

    let envs = std::env::vars().chain([env]).collect::<Vec<_>>();

    let mut cmd = Command::new("git");
    cmd.args([rpc, "--stateless-rpc", path])
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
    Path((owner, repo)): Path<(String, String)>,
    State(AppState { base, .. }): State<AppState>,
    Query(GitQuery { service }): Query<GitQuery>,
    headers: HeaderMap,
) -> Response<Body> {
    let service = get_git_service(&service);

    let path = match repo.ends_with(".git") {
        true => format!("{base}/{owner}/{repo}"),
        false => format!("{base}/{owner}/{repo}.git"),
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

    let envs = std::env::vars().chain([env]).collect::<Vec<_>>();

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
