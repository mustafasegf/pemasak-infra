use axum::{
    extract::State,
    response::{Html, Response},
    routing::{get, post},
    Form, Router,
};
use hyper::Body;
use leptos::*;
use secrecy::{ExposeSecret, Secret};
use serde::Deserialize;
use ulid::Ulid;
use uuid::Uuid;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

use crate::{configuration::Settings, startup::AppState};

pub fn router(state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route("/api/user/register", post(register_user_api))
        .route("/user/register", get(register_user_ui).post(register_user))
        .with_state(state)
}

#[derive(Deserialize)]
pub struct UserRequest {
    pub username: String,
    pub name: String,
    pub password: Secret<String>,
}

#[tracing::instrument(skip(pool, password))]
pub async fn register_user(
    State(AppState { pool, .. }): State<AppState>,
    Form(UserRequest {
        username,
        name,
        password,
    }): Form<UserRequest>,
) -> Html<String> {
    // check if user exists

    // TODO: return appropriate body
    match sqlx::query!(
        r#"SELECT username FROM users WHERE username = $1"#,
        username
    )
    .fetch_one(&pool)
    .await
    {
        Err(sqlx::Error::RowNotFound) => {}
        // TODO: change this into enum error and do early return
        Err(err) => {
            tracing::error!("Failed to query database: {}", err);
            let html = leptos::ssr::render_to_string(move || {
                view! {
                    <h1> Failed to query database {err.to_string() } </h1>
                }
            });
            return Html(html.into_owned());
        }
        Ok(_) => {
            let html = leptos::ssr::render_to_string(|| {
                view! {
                    <h1> User already exists </h1>
                }
            });
            return Html(html.into_owned());
        }
    }

    let id = Uuid::from(Ulid::new());
    let hasher = Argon2::default();
    let salt = SaltString::generate(&mut OsRng);
    let password = match hasher.hash_password(password.expose_secret().as_bytes(), &salt) {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!("Failed to hash password: {}", err);
            let html = leptos::ssr::render_to_string(move || {
                view! {
                    <h1> Failed to hash password {err.to_string() } </h1>
                }
            });
            return Html(html.into_owned());
        }
    };

    match sqlx::query!(
        r#"INSERT INTO users (id, username, password, name) VALUES ($1, $2, $3, $4)"#,
        id,
        username,
        password.to_string(),
        name
    )
    .execute(&pool)
    .await
    {
        Err(err) => {
            tracing::error!("Failed to insert into database: {}", err);
            let html = leptos::ssr::render_to_string(move || {
                view! {
                    <h1> Failed to insert into database {err.to_string() } </h1>
                }
            });
            Html(html.into_owned())
        }
        Ok(_) => {
            let html = leptos::ssr::render_to_string(|| {
                view! {
                    <h1> User created </h1>
                }
            });
            Html(html.into_owned())
        }
    }
}

#[tracing::instrument]
pub async fn register_user_ui() -> Html<String> {
    let html = leptos::ssr::render_to_string(|| {
        view! {
            <html>
                <head>
                  <script src="https://unpkg.com/htmx.org@1.9.6"></script>
                // TODO: change tailwind to use node
                <link href="https://cdn.jsdelivr.net/npm/daisyui@3.8.2/dist/full.css" rel="stylesheet" type="text/css" />
    <script src="https://cdn.tailwindcss.com"></script>

                </head>
                <body>
                    <h1> Register </h1>
                    <form 
                      hx-post="/user/register" 
                      hx-trigger="submit"
                      hx-target="#result"
                      hx-swap="outerHTML"
                      class="card-body"
                    >
                        <label for="username">Username</label>
                        <input type="text" name="username" id="username" required class="input input-bordered w-full max-w-xs" />
                        <label for="name">Name</label>
                        <input type="text" name="name" id="name" required class="input input-bordered w-full max-w-xs" />
                        <label for="password">Password</label>
                        <input type="password" name="password" id="password" required class="input input-bordered w-full max-w-xs" />
                        <button type="submit" class="btn">Register</button>
                    </form>
                    <div id="result"></div>
                </body>
            </html>
        }
    })
    .into_owned();

    Html(html)
}

#[tracing::instrument(skip(pool, password))]
pub async fn register_user_api(
    State(AppState { pool, .. }): State<AppState>,
    Form(UserRequest {
        username,
        name,
        password,
    }): Form<UserRequest>,
) -> Response<Body> {
    // check if user exists

    // TODO: return appropriate body
    match sqlx::query!(
        r#"SELECT username FROM users WHERE username = $1"#,
        username
    )
    .fetch_one(&pool)
    .await
    {
        Err(sqlx::Error::RowNotFound) => {}
        // TODO: change this into enum error and do early return
        Err(err) => {
            tracing::error!("Failed to query database: {}", err);
            return Response::builder().status(500).body(Body::empty()).unwrap();
        }
        Ok(_) => {
            return Response::builder().status(409).body(Body::empty()).unwrap();
        }
    }

    let id = Uuid::from(Ulid::new());
    let hasher = Argon2::default();
    let salt = SaltString::generate(&mut OsRng);
    let password = match hasher.hash_password(password.expose_secret().as_bytes(), &salt) {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!("Failed to hash password: {}", err);
            return Response::builder().status(500).body(Body::empty()).unwrap();
        }
    };

    match sqlx::query!(
        r#"INSERT INTO users (id, username, password, name) VALUES ($1, $2, $3, $4)"#,
        id,
        username,
        password.to_string(),
        name
    )
    .execute(&pool)
    .await
    {
        Err(err) => {
            tracing::error!("Failed to insert into database: {}", err);
            Response::builder().status(500).body(Body::empty()).unwrap()
        }
        Ok(_) => Response::builder().body(Body::empty()).unwrap(),
    }
}
