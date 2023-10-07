use std::fs::File;

use axum::{
    extract::{State, Path},
    response::{Html, Response},
    routing::{get, delete},
    Form, Router,
};
use axum_extra::routing::RouterExt;
use serde_json::json;
use axum_session::SessionPgPool;
use axum_session_auth::AuthSession;
use hyper::{Body, StatusCode};
use leptos::{*, ssr::render_to_string};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use ulid::Ulid;
use uuid::Uuid;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use rand::{Rng, SeedableRng};

use crate::{startup::AppState, configuration::Settings, auth::User, components::Base};

// Base64 url safe
const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const TOKEN_LENGTH: usize = 32;


pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route("/new", get(create_project_ui).post(create_project))
        .route_with_tsr("/:owner/:project", delete(delete_project_api))
}


// TODO: we need to finalize the working between repo and project
#[derive(Deserialize)]
pub struct RepoRequest {
    pub owner: String,
    pub project: String,
}

#[tracing::instrument(skip(auth, pool, base, domain))]
pub async fn create_project(
    auth: AuthSession<User, Uuid, SessionPgPool, PgPool>,
    State(AppState { pool, base, domain, .. }): State<AppState>,
    Form(RepoRequest {
        owner,
        project,
    }): Form<RepoRequest>,
) -> Html<String> {
    //check auth
    if auth.current_user.is_none() {
        return Html(render_to_string(|| { view! {
            <h1> User not authenticated </h1>
        }}).into_owned());
    };

    // validate project name
    if project.contains(char::is_whitespace) {
        return Html(render_to_string(|| { view! {
            <h1> Project name cannot contain whitespace </h1>
        }}).into_owned());
    }

    let path = match project.ends_with(".git") {
        true => format!("{base}/{owner}/{project}"),
        false => format!("{base}/{owner}/{project}.git"),
    };

    // check if owner exist
    let owner_id = match sqlx::query!(
        r#"SELECT id FROM project_owners WHERE name = $1 AND deleted_at IS NULL"#,
        owner,
    ).fetch_one(&pool).await {
        Ok(data) => data.id,
        Err(err) => {
            tracing::error!("Failed to query database: {}", err);
            return Html(render_to_string(move || { view! {
                <h1> Failed to query database {err.to_string() } </h1>
            }}).into_owned());
        }
    };

    // check if project already exist
    match sqlx::query!(
        r#"SELECT id FROM projects WHERE name = $1 AND owner_id = $2"#,
        project,
        owner_id,
    ).fetch_one(&pool).await {
        Err(sqlx::Error::RowNotFound) => {},
        Ok(_) => {
            return Html(render_to_string(move || { view! {
                <h1> Project already exist</h1>
            }}).into_owned());
        },
        Err(err) => {
            tracing::error!("Failed to query database: {}", err);
            return Html(render_to_string(move || { view! {
                <h1> Failed to query database {err.to_string() } </h1>
            }}).into_owned());
        }
    }

    // create project
    let project_id = match sqlx::query!(
        r#"INSERT INTO projects (id, name, owner_id) VALUES ($1, $2, $3) RETURNING id"#,
        Uuid::from(Ulid::new()),
        project,
        owner_id,
    ).fetch_one(&pool).await {
        Ok(data) => data.id,
        Err(err) => {
            tracing::error!("Failed to insert into database: {}", err);
            return Html(render_to_string(move || { view! {
                <h1> Failed to insert into database {err.to_string() } </h1>
            }}).into_owned());
        }
    };

    // if File::open(&path).is_ok() {
    //     return Html(render_to_string(|| { view! {
    //         <h1> project name already taken </h1>
    //     }}).into_owned());
    // };

    if let Err(err) =  git2::Repository::init_bare(path) {
        tracing::error!("Failed to create repo: {}", err);
        return Html(render_to_string(move || { view! {
            <h1> Failed to query database {err.to_string() } </h1>
        }}).into_owned());
    }

    // generate token
    let mut rng = rand::rngs::StdRng::from_entropy();
    let token = (0..TOKEN_LENGTH).map(|_| {
        let idx = rng.gen_range(0..CHARSET.len());
        CHARSET[idx] as char
    }).collect::<String>();

    let salt = SaltString::generate(&mut OsRng);
    // TODO: check if we can move this into app state
    let hasher = Argon2::default();
    let hash = match hasher.hash_password(token.as_bytes(), &salt) {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!("Failed to hash token: {}", err);
            return Html(render_to_string(move || { view! {
                <h1> Failed to generate token {err.to_string() } </h1>
            }}).into_owned());
        }
    };

    if let Err(e) = sqlx::query!(
        r#"INSERT INTO api_token (id, project_id, token) VALUES ($1, $2, $3)"#,
        Uuid::from(Ulid::new()),
        project_id,
        hash.to_string(),
    ).execute(&pool).await {
        tracing::error!("Failed to insert into database: {}", e);
        return Html(render_to_string(move || { view! {
            <h1> Failed to insert into database {e.to_string() } </h1>
        }}).into_owned());
    };

    Html(render_to_string(move || { view! {
        <h1> Project created successfully  </h1>
        <div class="p-4 bg-gray-800">
            <pre><code id="code"> 
                git remote add origin {format!(" http://{domain}/{owner}/{project}")} <br/>
                {"git push -u origin master"}
            </code></pre>
        </div>
        project token: {token}
        <button
        class="btn btn-outline btn-secondary mt-4"
        onclick="
            let lb = '\\n'
            if(navigator.userAgent.indexOf('Windows') != -1) {{
              lb = '\\r\\n'
            }}

            let text = document.getElementById('code').getInnerHTML().replaceAll('<br>', lb)
            if ('clipboard' in window.navigator) {{
                navigator.clipboard.writeText(text)
            }}"
        > Copy to clipboard </button>
    }}).into_owned())
}

#[derive(sqlx::Type, Eq, PartialEq, Deserialize, Serialize, Debug)]
pub struct Owner {
    pub id: Uuid,
    pub name: String,
}

#[derive(Eq, PartialEq, Deserialize, Serialize, Debug)]
pub struct UserOwner {
    pub id: Uuid,
    pub name: String,
    pub username: String,
    pub owners: Vec<Owner>,
}

#[tracing::instrument(skip(auth, pool))]
pub async fn create_project_ui(
    auth: AuthSession<User, Uuid, SessionPgPool, PgPool>,
    State(AppState { pool, .. }): State<AppState>,
) -> Response<Body> {
    // TODO: move this logic to middleware
    let user = match auth.current_user {
        Some(user) => user,
        None => {
            return Response::builder().status(StatusCode::FOUND).header("Location", "/login").body(Body::empty()).unwrap();
        }
    };

    let user_owners: UserOwner = match sqlx::query_as!(
        UserOwner,
        r#"SELECT 
            u.id, 
            u.name, 
            u.username,
            COALESCE(NULLIF(ARRAY_AGG((o.id, o.name)), '{NULL}'), '{}') AS "owners!: Vec<Owner>" 
          FROM users u, project_owners o
          WHERE o.deleted_at is NULL
            AND u.id = $1
          GROUP BY u.id"#,
        user.id
    ).fetch_one(&pool).await {
        Ok(data) => data,
        Err(err) => {
            tracing::error!("Failed to query database: {}", err);
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to query database {err.to_string() } </h1>
                }
            }).into_owned();
            return Response::builder().status(500).body(Body::from(html)).unwrap();
        }
    };

    let html = render_to_string(move || view! {
        <Base>
            <form 
              hx-post="/new" 
              hx-trigger="submit"
              hx-target="#result"
              class="flex flex-col mb-4 gap-2"
            >
                <h1 class="text-2xl font-bold"> Create Project </h1>
                <h3 class="text-lg"> {format!("login as {}", user.username)} </h3>
                <div class="flex flex-row gap-2">
                    <div class="form-control">
                      <label class="label">
                        <span class="label-text">Owner</span>
                      </label>
                        <select name="owner" class="select select-bordered w-full max-w-xs">
                            {user_owners.owners.into_iter().map(|owner|{ view!{ 
                                <option>{owner.name}</option>
                            }}).collect::<Vec<_>>()}
                        </select>
                    </div>
                    <div class="form-control">
                      <label class="label">
                        <span class="label-text">Project</span>
                      </label>
                        <input type="text" name="project" required class="input input-bordered w-full max-w-xs"/>
                    </div>
                </div>
                <button class="mt-4 btn btn-primary w-full max-w-xs">Create Project</button>
            </form>
            <div id="result"></div>
        </Base>
    }).into_owned();
    Response::builder().status(StatusCode::OK).header("Content-Type", "text/html").body(Body::from(html)).unwrap()
}

// pub async fn create_project_api(
//     Path((owner, repo)): Path<(String, String)>,
//     State(AppState { pool, base, .. }): State<AppState>,
// ) -> Response<Body> {
//     // check if repo exists
//     let path = match repo.ends_with(".git") {
//         true => format!("{base}/{owner}/{repo}"),
//         false => format!("{base}/{owner}/{repo}.git"),
//     };
//
//     // check
//
//     if File::open(&path).is_ok() {
//         return Response::builder()
//             .status(StatusCode::CONFLICT)
//             .body(Body::from(json!({"message": "repo exist"}).to_string()))
//             .unwrap();
//     };
//
//     match git2::Repository::init_bare(&path) {
//         Ok(_) => Response::builder()
//             .body(Body::from(
//                 json!({"message": "repo created successfully"}).to_string(),
//             ))
//             .unwrap(),
//         Err(e) => Response::builder()
//             .status(StatusCode::INTERNAL_SERVER_ERROR)
//             .body(Body::from(
//                 json!({"message": format!("failed to init repo: {}", e)}).to_string(),
//             ))
//             .unwrap(),
//     }
// }

pub async fn delete_project_api(
    Path((owner, project)): Path<(String, String)>,
    State(AppState { pool, base, .. }): State<AppState>,
) -> Response<Body> {
    let path = match project.ends_with(".git") {
        true => format!("{base}/{owner}/{project}"),
        false => format!("{base}/{owner}/{project}.git"),
    };

    // check if owner exist
    let owner_id = match sqlx::query!(
        r#"SELECT id FROM project_owners WHERE name = $1 AND deleted_at IS NULL"#,
        owner,
    ).fetch_one(&pool).await {
        Ok(data) => data.id,
        Err(err) => {
            tracing::error!("Failed to query database: {}", err);
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(
                    json!({"message": "failed to query database"}).to_string(),
                ))
                .unwrap();
        }
    };

    // check if project
    match sqlx::query!(
        r#"SELECT id FROM projects WHERE name = $1 AND owner_id = $2"#,
        project,
        owner_id,
    ).fetch_one(&pool).await {
        Err(sqlx::Error::RowNotFound) => {},
        Ok(_) => {
            return Response::builder()
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .body(Body::from(
                    json!({"message": "project doesn't exist"}).to_string(),
                ))
                .unwrap();
        },
        Err(err) => {
            tracing::error!("Failed to query database: {}", err);
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(
                    json!({"message": "failed to query database"}).to_string(),
                ))
                .unwrap();
        }
    }
    // check if repo exists
    if File::open(&path).is_err() {
        return Response::builder()
            .status(StatusCode::UNPROCESSABLE_ENTITY)
            .body(Body::from(
                json!({"message": "repo doesn't exist"}).to_string(),
            ))
            .unwrap();
    };

    match std::fs::remove_dir_all(&path) {
        Ok(_) => Response::builder()
            .body(Body::from(
                json!({"message": "repo deleted successfully"}).to_string(),
            ))
            .unwrap(),
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(
                json!({"message": format!("failed to delete repo: {}", e)}).to_string(),
            ))
            .unwrap(),
    }
}
