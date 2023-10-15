use std::fs::File;

use axum::{
    extract::{State, Path},
    response::Response,
    routing::{get, delete},
    Form, Router, middleware,
};
use axum_extra::routing::RouterExt;
use garde::{Validate, Unvalidated};
use serde_json::json;
use hyper::{Body, StatusCode};
use leptos::{*, ssr::render_to_string};
use serde::Deserialize;
use ulid::Ulid;
use uuid::Uuid;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use rand::{Rng, SeedableRng};

use crate::{startup::AppState, configuration::Settings, auth::{auth, Auth}, components::Base};

// Base64 url safe
const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const TOKEN_LENGTH: usize = 32;


pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route("/new", get(create_project_ui).post(create_project))
        .route("/dashboard", get(dashboard_ui).post(create_project))
        .route_with_tsr("/:owner/:project", delete(delete_project_api))
        .route_layer(middleware::from_fn(auth))
        
}

#[derive(Deserialize, Validate, Debug)]
pub struct CreateProjectRequest {
    #[garde(length(min=1))]
    pub owner: String,
    #[garde(alphanumeric)]
    pub project: String,
}

#[tracing::instrument(skip(pool, base, domain))]
pub async fn create_project(
    State(AppState { pool, base, domain, .. }): State<AppState>,
    Form(req): Form<Unvalidated<CreateProjectRequest>>,
) -> Response<Body> {

    let CreateProjectRequest{ owner, project } = match req.validate(&()){
        Ok(valid) => valid.into_inner(),
        Err(err) => {
            let html = render_to_string(move || { view! {
                <p> {err.to_string() } </p>
            }}).into_owned();
            return Response::builder().status(StatusCode::BAD_REQUEST).body(Body::from(html)).unwrap();
        }
    };

    let path = match project.ends_with(".git") {
        true => format!("{base}/{owner}/{project}"),
        false => format!("{base}/{owner}/{project}.git"),
    };

    // check if owner exist
    let owner_id = match sqlx::query!(
        r#"SELECT id FROM project_owners WHERE name = $1 AND deleted_at IS NULL"#,
        owner,
    ).fetch_optional(&pool).await {
        Ok(Some(data)) => data.id,
        Ok(None) => {
            let html = render_to_string(move || { view! {
                <h1> Owner does not exist </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::BAD_REQUEST).body(Body::from(html)).unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get project_owners: Failed to query database");
            let html = render_to_string(move || { view! {
                <h1> Failed to query database {err.to_string() } </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
        }
    };

    // check if project already exist
    match sqlx::query!(
        r#"SELECT id FROM projects WHERE name = $1 AND owner_id = $2"#,
        project,
        owner_id,
    ).fetch_optional(&pool).await {
        Ok(None) => {},
        Ok(_) => {
            let html = render_to_string(move || { view! {
                <h1> Project already exist</h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::CONFLICT).body(Body::from(html)).unwrap();

        },
        Err(err) => {
            tracing::error!(?err, "Can't get projects: Failed to query database");
            let html = render_to_string(move || { view! {
                <h1> Failed to query database {err.to_string() } </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
        }
    }

    // TODO: create this into a tx and rollback if failed to create git repo
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!(?err, "Can't insert user: Failed to begin transaction");
            let html = render_to_string(move || { view! {
                <h1> Failed to begin transaction {err.to_string() } </h1>
            }}).into_owned();

            return Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
        }
    };

    // create project
    let project_id = match sqlx::query!(
        r#"INSERT INTO projects (id, name, owner_id) VALUES ($1, $2, $3) RETURNING id"#,
        Uuid::from(Ulid::new()),
        project,
        owner_id,
    ).fetch_one(&mut *tx).await {
        Ok(data) => data.id,
        Err(err) => {
            tracing::error!(?err, "Can't insert projects: Failed to insert into database");
            if let Err(err) = tx.rollback().await {
                tracing::error!(?err, "Can't insert projects: Failed to rollback transaction");
            }

            let html = render_to_string(move || { view! {
                <h1> Failed to insert into database</h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
        }
    };

    if let Err(err) =  git2::Repository::init_bare(path) {
        tracing::error!(?err, "Can't create project: Failed to create repo");
        let html = render_to_string(move || { view! {
            <h1> Failed to create project: {err.to_string() } </h1>
        }}).into_owned();
        return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
    }

    // generate token
    let mut rng = rand::rngs::StdRng::from_entropy();
    let token = (0..TOKEN_LENGTH).map(|_| {
        let idx = rng.gen_range(0..CHARSET.len());
        CHARSET[idx] as char
    }).collect::<String>();

    let salt = SaltString::generate(&mut OsRng);
    let hasher = Argon2::default();
    let hash = match hasher.hash_password(token.as_bytes(), &salt) {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!(?err, "Can't create project: Failed to hash token");
            let html = render_to_string(move || { view! {
                <h1> Failed to generate token {err.to_string() } </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
        }
    };

    if let Err(err) = sqlx::query!(
        "INSERT INTO api_token (id, project_id, token) VALUES ($1, $2, $3)",
        Uuid::from(Ulid::new()),
        project_id,
        hash.to_string(),
    ).execute(&mut *tx).await {
        tracing::error!(?err, "Can't insert api_token: Failed to insert into database");
        let html = render_to_string(move || { view! {
            <h1> Failed to insert into database {err.to_string() } </h1>
        }}).into_owned();
        return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
    };


    if let Err(err) = tx.commit().await {
        tracing::error!(?err, "Can't create project: Failed to commit transaction");
        let html = render_to_string(move || { view! {
            <h1> Failed to commit transaction {err.to_string() } </h1>
        }}).into_owned();
        return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Body::from(html)).unwrap();
    }

    let html = render_to_string(move || { view! {
        <h1> Project created successfully  </h1>
        <div class="p-4 mb-4 bg-gray-800">
            <pre><code id="code"> 
                git remote add origin {format!(" http://{domain}/{owner}/{project}")} <br/>
                {"git push -u origin master"}
            </code></pre>
        </div>
        <button
            class="btn btn-outline btn-secondary mb-4"
            onclick="
                let lb = '\\n'
                if(navigator.userAgent.indexOf('Windows') != -1) {{
                  lb = '\\r\\n'
                }}

                let text = document.getElementById('code').getInnerHTML().replaceAll('<br>', lb)
                if ('clipboard' in window.navigator) {{
                    navigator.clipboard.writeText(text)
                }}"
        >
          Copy to clipboard 
        </button>

        <div class="p-4 mb-4 bg-gray-800">
            <pre><code> 
              project token: <span id="token">{token} </span>
            </code></pre>
        </div>
        <button
            class="btn btn-outline btn-secondary"
            onclick="
                let text = document.getElementById('token').innerText
                if ('clipboard' in window.navigator) {{
                    navigator.clipboard.writeText(text)
                }}"
        >
          Copy to clipboard 
        </button>
    }}).into_owned();

    Response::builder().status(StatusCode::OK).body(Body::from(html)).unwrap()
}

#[tracing::instrument(skip(auth, pool))]
pub async fn create_project_ui(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
) -> Response<Body> {
    let user = auth.current_user.unwrap();

    let owners = match sqlx::query!(
        r#"select o.id, o.name
           FROM project_owners o
           JOIN users_owners uo on uo.owner_id = o.id
           where uo.user_id = $1
           AND o.deleted_at is NULL
        "#,
        user.id
    ).fetch_all(&pool).await {
        Ok(data) => data,
        Err(err) => {
            tracing::error!(?err, "Can't get owners: Failed to query database");
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
                <h3 class="text-lg"> "login as " {user.username} </h3>
                <div class="flex flex-row gap-2">
                    <div class="form-control">
                      <label class="label">
                        <span class="label-text">Owner</span>
                      </label>
                        <select name="owner" class="select select-bordered w-full max-w-xs">
                            {owners.into_iter().map(|owner|{ view!{ 
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


#[tracing::instrument(skip(auth, pool))]
pub async fn dashboard_ui(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
) -> Response<Body> {
    let user = auth.current_user.unwrap();

    let projects = match sqlx::query!(
        r#"SELECT projects.name AS project, project_owners.name AS owner
           FROM projects
           JOIN project_owners ON projects.owner_id = project_owners.id
           JOIN users_owners ON project_owners.id = users_owners.owner_id
           JOIN users ON users_owners.user_id = users.id
           WHERE users.id = $1
        "#,
        user.id
    ).fetch_all(&pool).await {
        Ok(data) => data,
        Err(err) => {
            tracing::error!(?err, "Can't get projects: Failed to query database");
            let html = render_to_string(move || {
                view! {
                    <h1> "Failed to query database "{err.to_string() } </h1>
                }
            }).into_owned();
            return Response::builder().status(500).body(Body::from(html)).unwrap();
        }
    };

    let html = render_to_string(move || view! {
        <Base>
            <h1 class="text-2xl font-bold">Your Projects</h1>
            <h3 class="text-lg">"login as " {user.username}</h3>
            <div hx-boost="true" class="flex flex-col gap-4">
                {projects.into_iter().map(|record|{ view!{ 
                <div class="bg-neutral text-info py-4 px-8 w-full">
                    {let name = format!("{}/{}", record.owner, record.project);
                    view!{<a href={name.clone()} class="text-sm">{name}</a>}}
                </div>
                }}).collect::<Vec<_>>()}
                <a href="/new" class="mt-4 btn btn-primary w-full max-w-xs">Create Project</a>
            </div>
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
    ).fetch_optional(&pool).await {
        Ok(Some(data)) => data.id,
        Ok(None) => {
            tracing::debug!("Can't delete project: Owner does not exist");
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(
                    json!({"message": "owner does not exist"}).to_string(),
                ))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get project_owners: Failed to query database");
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(
                    json!({"message": "failed to query database"}).to_string(),
                ))
                .unwrap();
        }
    };

    // check if project exist
    let mut errs = match sqlx::query!(
        r#"SELECT id FROM projects WHERE name = $1 AND owner_id = $2"#,
        project,
        owner_id,
    ).fetch_one(&pool).await {
        Ok(_) => {
            match sqlx::query!("DELETE FROM projects WHERE name = $1 AND owner_id = $2", project, owner_id)
                .execute(&pool)
                .await 
            {
                Ok(_) => vec![],
                Err(err) => vec![anyhow::anyhow!("failed to delete project: {}", err)]
            }
        },
        Err(sqlx::Error::RowNotFound) => vec![],
        Err(_err) => vec![anyhow::anyhow!("failed to query database")],
    };
    
    // check if repo exists
    match File::open(&path) {
        Err(e) => errs.push(anyhow::anyhow!("failed to open repo: {}", e)),
        Ok(_) => {
            match std::fs::remove_dir_all(&path) {
                Ok(_) => {},
                Err(e) => errs.push(anyhow::anyhow!("failed to delete repo: {}", e)),
            }
        },
    };

    match errs.as_slice() {
        [] => {
        Response::builder()
            .body(Body::from(
                json!({"message": "repo deleted successfully"}).to_string(),
            ))
            .unwrap()
        }
        errs => {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(
                    json!({"message": format!("failed to delete project: {:?}", errs)}).to_string(),
                ))
                .unwrap()
        }
    }
}
