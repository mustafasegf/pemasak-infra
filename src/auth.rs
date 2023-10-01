use std::collections::HashSet;

use axum::{
    extract::State,
    response::{Html, Response},
    routing::{get, post},
    Form, Router,
};
use axum_session::{SessionConfig, SessionLayer, SessionStore};
use hyper::Body;
use leptos::{*, ssr::render_to_string};
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use ulid::Ulid;
use uuid::Uuid;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

use axum_session_auth::*;
use async_trait::async_trait;

use crate::{configuration::Settings, startup::AppState};

pub async fn router(state: AppState, _config: &Settings) -> Router<AppState, Body> {
    let pool = state.pool.clone();
    let session_config = SessionConfig::default().with_table_name("axum_sessions");
    let auth_config = AuthConfig::<Uuid>::default().with_anonymous_user_id(Some(Uuid::default()));
        let session_store =
        SessionStore::<SessionPgPool>::new(Some(pool.clone().into()), session_config)
            .await
            .unwrap();


    Router::new()
        .route("/api/user/register", post(register_user_api))
        .route("/user/register", get(register_user_ui).post(register_user))
        .route("/user/login", get(login_user_ui).post(login_user))
        .with_state(state)
                .layer(
            AuthSessionLayer::<User, Uuid, SessionPgPool, PgPool>::new(Some(pool.clone()))
                .with_config(auth_config),
        )
        .layer(SessionLayer::new(session_store))

}


#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password: String,
    pub permissions: HashSet<String>,
}

impl Default for User {
    fn default() -> Self {
        let permissions = HashSet::new();

        Self {
            id: Uuid::default(),
            username: "Guest".into(),
            password: "".into(),
            permissions,
        }
    }
}

impl User {
    pub async fn get(id: Uuid, pool: &PgPool) -> Option<Self> {
        let sqluser = sqlx::query_as::<_, SqlUser>("SELECT * FROM users WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .ok()?;

        //lets just get all the tokens the user can use, we will only use the full permissions if modifing them.
        let sql_user_perms = sqlx::query_as::<_, SqlPermissionTokens>(
            "SELECT token FROM user_permissions WHERE user_id = $1;",
        )
        .bind(id)
        .fetch_all(pool)
        .await
        .ok()?;

        Some(sqluser.into_user(Some(sql_user_perms)))
    }

    pub async fn get_from_username(name: String, pool: &PgPool) -> Option<Self> {
        let sqluser = sqlx::query_as::<_, SqlUser>("SELECT * FROM users WHERE username = $1")
            .bind(name)
            .fetch_one(pool)
            .await
            .map_err(|err| {
                tracing::error!("Failed to get user from username: {}", err);
                err
            })
            .ok()?;

        tracing::info!("Got user from username: {:?}", sqluser);

        //lets just get all the tokens the user can use, we will only use the full permissions if modifing them.
        let sql_user_perms = sqlx::query_as::<_, SqlPermissionTokens>(
            "SELECT token FROM user_permissions WHERE user_id = $1;",
        )
        .bind(sqluser.id)
        .fetch_all(pool)
        .await
            .map_err(|err| {
                tracing::error!("Failed to get user permissions: {}", err);
                err
            })
        .ok()?;

        Some(sqluser.into_user(Some(sql_user_perms)))
    }
}

#[derive(sqlx::FromRow, Clone)]
pub struct SqlPermissionTokens {
    pub token: String,
}

#[async_trait]
impl Authentication<User, Uuid, PgPool> for User {
    async fn load_user(userid: Uuid, pool: Option<&PgPool>) -> Result<User, anyhow::Error> {
        let pool = pool.unwrap();

        User::get(userid, pool)
            .await
            .ok_or_else(|| anyhow::anyhow!("Cannot get user"))
    }

    fn is_authenticated(&self) -> bool {
        true
    }

    fn is_active(&self) -> bool {
        true
    }

    fn is_anonymous(&self) -> bool {
        false
    }
}

#[async_trait]
impl HasPermission<PgPool> for User {
    async fn has(&self, perm: &str, _pool: &Option<&PgPool>) -> bool {
        self.permissions.contains(perm)
    }
}

#[derive(sqlx::FromRow, Clone, Debug)]
pub struct SqlUser {
    pub id: Uuid,
    pub username: String,
    pub password: String,
}

impl SqlUser {
    pub fn into_user(self, sql_user_perms: Option<Vec<SqlPermissionTokens>>) -> User {
        User {
            id: self.id,
            username: self.username,
            password: self.password,
            permissions: if let Some(user_perms) = sql_user_perms {
                user_perms
                    .into_iter()
                    .map(|x| x.token)
                    .collect::<HashSet<String>>()
            } else {
                HashSet::<String>::new()
            },
        }
    }
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
    match sqlx::query!(
        r#"SELECT username FROM users WHERE username = $1"#,
        username
    )
    .fetch_one(&pool)
    .await
    {
        Err(sqlx::Error::RowNotFound) => {}
        Err(err) => {
            tracing::error!("Failed to query database: {}", err);
            return Html(render_to_string(move || { view! {
                <h1> Failed to query database {err.to_string() } </h1>
            }}).into_owned());
        }

        Ok(_) => {
            return Html(render_to_string(|| { view! {
                <h1> User already exists </h1>
            }}).into_owned());
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


#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: Secret<String>,
}

#[tracing::instrument(skip(auth, pool, password))]
pub async fn login_user(
    auth: AuthSession<User, Uuid, SessionPgPool, PgPool>,
    State(AppState { pool, .. }): State<AppState>,
    Form(LoginRequest {
        username,
        password,
    }): Form<LoginRequest>,
) -> Html<String> {
    // get user
    let user = match User::get_from_username(username, &pool).await {
        Some(user) => user,
        None => {
            return Html(render_to_string(|| { view! {
                <h1> User does not exist </h1>
            }}).into_owned());
        }
    };

    // check password
    let hasher = Argon2::default();
    let hash = PasswordHash::new(&user.password).unwrap();
    if let Err(err) = hasher.verify_password(password.expose_secret().as_bytes(), &hash) {
        tracing::error!("Failed to verify password: {}", err);
        return  Html(leptos::ssr::render_to_string(move || {
            view! {
                <h1> Failed to verify password {err.to_string() } </h1>
            }
        }).into_owned());
    };

    auth.login_user(user.id);
    let html = leptos::ssr::render_to_string(|| view! { <h1> success login </h1> }).into_owned();
    Html(html)
}

pub async fn login_user_ui() -> Html<String> {
    let html = leptos::ssr::render_to_string(|| view! {
        <html>
            <head>
              <script src="https://unpkg.com/htmx.org@1.9.6"></script>
            // TODO: change tailwind to use node
            <link href="https://cdn.jsdelivr.net/npm/daisyui@3.8.2/dist/full.css" rel="stylesheet" type="text/css" />
<script src="https://cdn.tailwindcss.com"></script>

            </head>
            <body>
                <h1> Login </h1>
                <form 
                  hx-post="/user/login" 
                  hx-trigger="submit"
                  hx-target="#result"
                  class="card-body"
                >
                    <label for="username">Username</label>
                    <input type="text" name="username" id="username" required class="input input-bordered w-full max-w-xs" />
                    <label for="password">Password</label>
                    <input type="password" name="password" id="password" required class="input input-bordered w-full max-w-xs" />
                    <button type="submit" class="btn">Register</button>
                </form>
                <div id="result"></div>
            </body>
        </html>
    }).into_owned();
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
