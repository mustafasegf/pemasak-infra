use axum::{extract::State, response::Response, Form};
use garde::{Unvalidated, Validate};
use hyper::{Body, StatusCode};
use leptos::{ssr::render_to_string, *};
use serde::Deserialize;
use serde_json::json;
use ulid::Ulid;
use uuid::Uuid;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use rand::{Rng, SeedableRng};

use crate::{auth::Auth, components::Base, startup::AppState};

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

#[tracing::instrument(skip(pool, base, domain))]
pub async fn post(
    auth: Auth,
    State(AppState {
        pool,
        base,
        domain,
        secure,
        ..
    }): State<AppState>,
    Form(req): Form<Unvalidated<CreateProjectRequest>>,
) -> Response<Body> {
    let CreateProjectRequest { owner, project } = match req.validate(&()) {
        Ok(valid) => valid.into_inner(),
        Err(err) => {
            let html = render_to_string(move || {
                view! {
                    <p> {err.to_string() } </p>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(html))
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
            let html = render_to_string(move || {
                view! {
                    <h1> Owner does not exist </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(html))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get project_owners: Failed to query database");
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to query database {err.to_string() } </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(html))
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
            let html = render_to_string(move || {
                view! {
                    <h1> Project already exist</h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::CONFLICT)
                .body(Body::from(html))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get projects: Failed to query database");
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to query database {err.to_string() } </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(html))
                .unwrap();
        }
    }

    // TODO: create this into a tx and rollback if failed to create git repo
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!(?err, "Can't insert user: Failed to begin transaction");
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to begin transaction {err.to_string() } </h1>
                }
            })
            .into_owned();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }
    };

    let envs = json!({
        "PRODUCTION": true
    });

    // create project
    let project_id = match sqlx::query!(
        r#"INSERT INTO projects (id, name, owner_id, envs) VALUES ($1, $2, $3, $4) RETURNING id"#,
        Uuid::from(Ulid::new()),
        project,
        owner_id,
    envs,
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

            let html = render_to_string(move || {
                view! {
                    <h1> Failed to insert into database</h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(html))
                .unwrap();
        }
    };

    if let Err(err) = git2::Repository::init_bare(path) {
        tracing::error!(?err, "Can't create project: Failed to create repo");
        let html = render_to_string(move || {
            view! {
                <h1> Failed to create project: {err.to_string() } </h1>
            }
        })
        .into_owned();
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(html))
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
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to generate token {err.to_string() } </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(html))
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
        let html = render_to_string(move || {
            view! {
                <h1> Failed to insert into database {err.to_string() } </h1>
            }
        })
        .into_owned();
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(html))
            .unwrap();
    };

    if let Err(err) = tx.commit().await {
        tracing::error!(?err, "Can't create project: Failed to commit transaction");
        let html = render_to_string(move || {
            view! {
                <h1> Failed to commit transaction {err.to_string() } </h1>
            }
        })
        .into_owned();
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(html))
            .unwrap();
    }

    let protocol = match secure {
        true => "https",
        false => "http",
    };

    let username = auth.current_user.unwrap().username;

    let html = render_to_string(move || {
        view! {
            <h1> Project created successfully  </h1>
            <div class="p-4 mt-4 bg-neutral/40 backdrop-blur-sm mockup-code" id="code">
                <pre>
                    <code>
                        "git remote add pws" {format!(" {protocol}://{domain}/{owner}/{project}")}
                    </code>
                </pre>
                <pre>
                    <code>
                        "git branch -M master" 
                    </code>
                </pre>
                <pre>
                    <code>
                        "git push pws master"
                    </code>
                </pre>
            </div>
            <button
                class="btn btn-outline btn-secondary mt-4"
                onclick="
                    let lb = '\\n'
                    if(navigator.userAgent.indexOf('Windows') != -1) {{
                    lb = '\\r\\n'
                    }}

                    let text = document.getElementById('code').innerText.replaceAll('\\n', lb)
                    if ('clipboard' in window.navigator) {{
                        navigator.clipboard.writeText(text)
                    }}
                "
            >
              Copy to clipboard
            </button>

            <div role="alert" class="alert alert-error mt-4">
                <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
                <span>MAKE SURE TO COPY THE CREDENTIAL BELOW AS YOU WILL NOT BE ABLE TO ACCESS IT AGAIN</span>
            </div>

            <div class="p-4 mt-4 bg-neutral/40 backdrop-blur-sm mockup-code" id="token">
                <pre><code>
                  {"Username: "}{username}
                </code></pre>
                <pre><code>
                  {"Password: "}{token}
                </code></pre>
            </div>
            <button
                class="btn btn-outline btn-secondary mt-4"
                onclick="
                let lb = '\\n'
                if(navigator.userAgent.indexOf('Windows') != -1) {{
                lb = '\\r\\n'
                }}

                let text = document.getElementById('token').innerText.replaceAll('\\n', lb)
                if ('clipboard' in window.navigator) {{
                    navigator.clipboard.writeText(text)
                }}"
            >
              Copy to clipboard
            </button>
        }
    })
    .into_owned();

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(html))
        .unwrap()
}

#[tracing::instrument(skip(auth, pool))]
pub async fn get(auth: Auth, State(AppState { pool, .. }): State<AppState>) -> Response<Body> {
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
