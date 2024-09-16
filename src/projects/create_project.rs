use axum::{
    extract::State,
    response::Response,
    Json,
};
use garde::{Unvalidated, Validate};
use hyper::{Body, StatusCode};
use leptos::{ssr::render_to_string, *};
use serde::{Deserialize, Serialize};
use ulid::Ulid;
use uuid::Uuid;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use rand::{Rng, SeedableRng};

use crate::{
    auth::Auth,
    components::Base,
    startup::AppState,
};

// Base64 url safe
const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const TOKEN_LENGTH: usize = 32;

#[derive(Deserialize, Validate, Debug)]
pub struct CreateProjectRequest {
    #[garde(length(min = 1))]
    pub owner: String,
    #[garde(alphanumeric)]
    pub project: String,
}

#[derive(Serialize, Debug)]
struct ErrorResponse {
    message: String
}

#[derive(Serialize, Debug)]
struct CreateProjectResponse {
    id: Uuid,
    owner_name: String,
    project_name: String,
    domain: String,
    git_username: String,
    git_password: String,
}

#[tracing::instrument(skip(pool, base, domain))]
pub async fn post(
    auth: Auth,
    State(AppState {
        pool, base, domain, secure, ..
    }): State<AppState>,
    Json(req): Json<Unvalidated<CreateProjectRequest>>,
) -> Response<Body> {    
    let CreateProjectRequest { owner, project } = match req.validate(&()) {
        Ok(valid) => valid.into_inner(),
        Err(err) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: err.to_string()
            }).unwrap();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(json))
                .unwrap();
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
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(data)) => data.id,
        Ok(None) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: "Owner does not exist".to_string()
            }).unwrap();
            
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(json))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get project_owners: Failed to query database");

            let json = serde_json::to_string(&ErrorResponse {
                message: format!("Failed to query database {}", err.to_string())
            }).unwrap();

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(json))
                .unwrap();
        }
    };

    // check if project already exist
    match sqlx::query!(
        r#"SELECT id FROM projects WHERE name = $1 AND owner_id = $2"#,
        project,
        owner_id,
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(None) => {}
        Ok(_) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: "Project already exists".to_string(),
            }).unwrap();

            return Response::builder()
                .status(StatusCode::CONFLICT)
                .body(Body::from(json))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get projects: Failed to query database");
            let json = serde_json::to_string(&ErrorResponse {
                message: format!("Failed to query database {}", err.to_string())
            }).unwrap();

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(json))
                .unwrap();
        }
    }

    // TODO: create this into a tx and rollback if failed to create git repo
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!(?err, "Can't insert user: Failed to begin transaction");

            let json = serde_json::to_string(&ErrorResponse {
                message: format!("Failed to begin transaction {}", err.to_string())
            }).unwrap();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
                .unwrap();
        }
    };

    // create project
    let project_id = match sqlx::query!(
        r#"INSERT INTO projects (id, name, owner_id) VALUES ($1, $2, $3) RETURNING id"#,
        Uuid::from(Ulid::new()),
        project,
        owner_id,
    )
    .fetch_one(&mut *tx)
    .await
    {
        Ok(data) => data.id,
        Err(err) => {
            tracing::error!(
                ?err,
                "Can't insert projects: Failed to insert into database"
            );
            if let Err(err) = tx.rollback().await {
                tracing::error!(
                    ?err,
                    "Can't insert projects: Failed to rollback transaction"
                );
            }

            let json = serde_json::to_string(&ErrorResponse {
                message: "Failed to insert into database".to_string()
            }).unwrap();

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(json))
                .unwrap();
        }
    };

    if let Err(err) = git2::Repository::init_bare(path) {
        tracing::error!(?err, "Can't create project: Failed to create repo");
        let json = serde_json::to_string(&ErrorResponse {
            message: format!("Failed to create project: {}", err.to_string())
        }).unwrap();

        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(json))
            .unwrap();
    }

    // generate token
    let mut rng = rand::rngs::StdRng::from_entropy();
    let token = (0..TOKEN_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect::<String>();

    let salt = SaltString::generate(&mut OsRng);
    let hasher = Argon2::default();
    let hash = match hasher.hash_password(token.as_bytes(), &salt) {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!(?err, "Can't create project: Failed to hash token");

            let json = serde_json::to_string(&ErrorResponse {
                message: format!("Failed to generate token {}", err.to_string())
            }).unwrap();
            
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(json))
                .unwrap();
        }
    };

    if let Err(err) = sqlx::query!(
        "INSERT INTO api_token (id, project_id, token) VALUES ($1, $2, $3)",
        Uuid::from(Ulid::new()),
        project_id,
        hash.to_string(),
    )
    .execute(&mut *tx)
    .await
    {
        tracing::error!(
            ?err,
            "Can't insert api_token: Failed to insert into database"
        );

        let json = serde_json::to_string(&ErrorResponse {
            message: format!("Failed to insert into database {}", err.to_string())
        }).unwrap();

        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(json))
            .unwrap();
    };

    if let Err(err) = tx.commit().await {
        tracing::error!(?err, "Can't create project: Failed to commit transaction");

        let json = serde_json::to_string(&ErrorResponse {
            message: format!("Failed to commit transaction: {}", err.to_string())
        }).unwrap();


        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(json))
            .unwrap();
    }

    let protocol = match secure {
        true => "https",
        false => "http",
    };

    let username = auth.current_user.unwrap().username;

    let json = serde_json::to_string(
        &CreateProjectResponse {
            id: project_id,
            owner_name: owner.clone(),
            project_name: project.clone(),
            domain: format!("{protocol}://{domain}/{owner}/{project}"),
            git_username: username,
            git_password: token,
        }
    ).unwrap();

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(json))
        .unwrap()
}

#[tracing::instrument(skip(auth, pool))]
pub async fn get(
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
    )
    .fetch_all(&pool)
    .await
    {
        Ok(data) => data,
        Err(err) => {
            tracing::error!(?err, "Can't get owners: Failed to query database");
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to query database {err.to_string() } </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(500)
                .body(Body::from(html))
                .unwrap();
        }
    };

    let html = render_to_string(move || view! {
        // TODO
        <Base is_logged_in={true}>
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
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html")
        .body(Body::from(html))
        .unwrap()
}
