use std::collections::HashSet;

use axum::{
    extract::State,
    response::{Html, Response},
    routing::get,
    Form, Router, middleware::Next,
};
use axum_session::{SessionConfig, SessionStore};
use bytes::Bytes;
use http_body::combinators::UnsyncBoxBody;
use hyper::{Body, StatusCode, Request};
use leptos::{*, ssr::render_to_string};
use regex::Regex;
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use ulid::Ulid;
use uuid::Uuid;

use garde::{Validate, Unvalidated};

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

use axum_session_auth::*;
use async_trait::async_trait;

use crate::{configuration::Settings, startup::AppState, components::Base};
use lazy_static::lazy_static;

lazy_static! {
    static ref USERNAME_REGEX: Regex = Regex::new(r"^[a-zA-Z0-9.]+$").unwrap();
}

pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route("/register", get(register_user_ui).post(register_user))
        .route("/login", get(login_user_ui).post(login_user))
        .route("/logout", get(logout_user).post(logout_user))
}

pub async fn auth<B>(
    auth: AuthSession<User, Uuid, SessionPgPool, PgPool>,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response<UnsyncBoxBody<Bytes, axum::Error>>, hyper::Response<Body>> {
    if auth.current_user.is_none() {
        return Err(Response::builder()
            .status(StatusCode::FOUND)
            .header("Location", "/login")
            .body(Body::empty())
            .unwrap());
    }

    Ok(next.run(request).await)
}

pub async fn auth_layer(pool: &PgPool) -> (AuthConfig<Uuid>, SessionStore<SessionPgPool>)  {

    let session_config = SessionConfig::default();
    let auth_config = AuthConfig::<Uuid>::default().with_anonymous_user_id(Some(Uuid::default()));

    let session_store =
    SessionStore::<SessionPgPool>::new(Some(pool.clone().into()), session_config)
        .await
        .unwrap();
    (auth_config, session_store)
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

// TODO: do we need this?
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

// bro idk why i need 3 borrows
fn password_check(value: &Secret<String>, _ctx: &&&()) -> garde::Result {
    if value.expose_secret().len() < 1 {
        return Err(garde::Error::new("Password cannot be empty"));
    }
    Ok(())
}

// why we use this and not the default garde regex match is becasue we need better error code until
// https://github.com/jprochazk/garde/issues/7 is merged
fn username_check(value: &String, _ctx: &&&()) -> garde::Result {
    if !USERNAME_REGEX.is_match(value) {
        return Err(garde::Error::new("Username can only contain alphanumeric characters and dots"));
    }
    Ok(())
}

#[derive(Deserialize,Validate, Debug)]
pub struct UserRequest {
    #[garde(custom(username_check))]
    pub username: String,
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(custom(password_check))]
    pub password: Secret<String>,
}

#[tracing::instrument(skip(auth, pool))]
pub async fn register_user(
    auth: AuthSession<User, Uuid, SessionPgPool, PgPool>,
    State(AppState { pool, .. }): State<AppState>,
    Form(req): Form<Unvalidated<UserRequest>>,
) -> Response<Body> {
    let UserRequest{ username, name, password } = match req.validate(&()){
        Ok(valid) => valid.into_inner(),
        Err(err) => {
            let html = render_to_string(move || { view! {
                <p> {err.to_string() } </p>
            }}).into_owned();
            return Response::builder().status(StatusCode::BAD_REQUEST).body(Body::from(html)).unwrap();
        }
    };

    // check if user exists
    match sqlx::query!("SELECT username FROM users WHERE username = $1", username)
        .fetch_one(&pool)
        .await
    {
        Err(sqlx::Error::RowNotFound) => {}
        Err(err) => {
            tracing::error!("Failed to query database: {}", err);
            let html = render_to_string(move || { view! {
                <h1> Failed to query database {err.to_string() } </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
        }

        Ok(_) => {
            let html = render_to_string(|| { view! {
                <h1> Username already exists </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
        }
    }

    // check if owner exists
    match sqlx::query!(
        r#"SELECT name FROM project_owners WHERE name = $1"#,
        username
    )
    .fetch_one(&pool)
    .await
    {
        Err(sqlx::Error::RowNotFound) => {}
        Err(err) => {
            tracing::error!("Failed to query database: {}", err);
            let html = render_to_string(move || { view! {
                <h1> Failed to query database {err.to_string() } </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
        }

        Ok(_) => {
            let html = render_to_string(|| { view! {
                <h1> Username already exists </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
        }
    }

    let user_id = Uuid::from(Ulid::new());
    let hasher = Argon2::default();
    let salt = SaltString::generate(&mut OsRng);

    let password = match hasher.hash_password(password.expose_secret().as_bytes(), &salt) {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!("Failed to hash password: {}", err);
            let html = render_to_string(move || { view! {
                <h1> Failed to hash password {err.to_string() } </h1>
            }}).into_owned();

            return Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!("Failed to begin transaction: {}", err);
            let html = render_to_string(move || { view! {
                <h1> Failed to begin transaction {err.to_string() } </h1>
            }}).into_owned();

            return Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
        }
    };

    // TODO: check if user from sso ui
    
    if let Err(err) = sqlx::query!(
        r#"INSERT INTO users (id, username, password, name) VALUES ($1, $2, $3, $4)"#,
        user_id,
        username,
        password.to_string(),
        name
    )
    .execute(&mut *tx)
    .await {
        tracing::error!("Failed to insert into database: {}", err);
        let errs = match tx.rollback().await {
            Ok(_) => vec![err],
            Err(e) => {
                    tracing::error!("Failed to rollback transaction: {}", e);
                    // TODO: check back if we need this
                    vec![err, e]
                },
        };
        let html = render_to_string(move || { view! {
            <h1> Failed to insert into database { 
                   errs.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ")
                 }
            </h1>
        }}).into_owned();
        
        return Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
    };

    let owner_id = Uuid::from(Ulid::new());

    if let Err(err) =  sqlx::query!(
        r#"INSERT INTO project_owners (id, name) VALUES ($1, $2)"#,
        owner_id,
        username
    )
    .execute(&mut *tx)
    .await {
        tracing::error!("Failed to insert into database: {}", err);
        let errs = match tx.rollback().await {
            Ok(_) => vec![err],
            Err(e) => {
                    tracing::error!("Failed to rollback transaction: {}", e);
                    // TODO: check back if we need this
                    vec![err, e]
                },
        };
        let html = render_to_string(move || { view! {
            <h1> Failed to insert into database { 
                   errs.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ")
                 }
            </h1>
        }}).into_owned();
        return Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
    };

    if let Err(err) = sqlx::query!(
            r#"INSERT INTO users_owners (user_id, owner_id) VALUES ($1, $2)"#,
            user_id,
            owner_id,
        )
        .execute(&mut *tx)
        .await {
        tracing::error!("Failed to insert into database: {}", err);
        let errs = match tx.rollback().await {
            Ok(_) => vec![err],
            Err(e) => {
                    tracing::error!("Failed to rollback transaction: {}", e);
                    // TODO: check back if we need this
                    vec![err, e]
                },
        };
        let html = render_to_string(move || { view! {
            <h1> Failed to insert into database { 
                   errs.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ")
                 }
            </h1>
        }}).into_owned();

        return Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
    }
    
    match tx.commit().await {
        Err(err) =>{
            tracing::error!("Failed to commit transaction: {}", err);
            let html = render_to_string(move || { view! {
                <h1> Failed to commit transaction {err.to_string() } </h1>
            }}).into_owned();
            Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap()
        }
        Ok(_) => {
            auth.login_user(user_id);
            let html = render_to_string(|| { view! {
                <h1> User created </h1>
            }}).into_owned();
            Response::builder().status(StatusCode::OK).header("Content-Type", "text/html").header("HX-Location", "/dashboard").body(Body::from(html)).unwrap()
        }
    }

}

#[tracing::instrument]
pub async fn register_user_ui(State(AppState { build_channel, .. }): State<AppState>,) -> Html<String> {
    let _ = build_channel.send(("A".to_string(), "B".to_string())).await;
    let html = render_to_string(|| {
        view! {
            <Base>
                <form 
                  hx-post="/register" 
                  hx-trigger="submit"
                  hx-target="#result"
                  class="flex flex-col mb-4 gap-1"
                >
                    <h1 class="text-2xl font-bold"> Register </h1>
                    <label for="username">Username</label>
                    <input type="text" name="username" id="username" required class="input input-bordered w-full max-w-xs" />
                    <label for="name">Name</label>
                    <input type="text" name="name" id="name" required class="input input-bordered w-full max-w-xs" />
                    <label for="password">Password</label>
                    <input type="password" name="password" id="password" required class="input input-bordered w-full max-w-xs" />
                    <button class="mt-4 btn btn-primary w-full max-w-xs">Register</button>
                </form>
                <div id="result"></div>
            </Base>
        }
    })
    .into_owned();

    Html(html)
}

#[tracing::instrument(skip(auth))]
pub async fn logout_user(
    auth: AuthSession<User, Uuid, SessionPgPool, PgPool>,
) -> Response<Body> {
    auth.logout_user();
    Response::builder().status(StatusCode::FOUND).header("Location", "/login").body(Body::empty()).unwrap()
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
) -> Response<Body> {
    // get user
    let user = match User::get_from_username(username, &pool).await {
        Some(user) => user,
        None => {
            let html = render_to_string(|| { view! {
                <h1> User does not exist </h1>
            }}).into_owned();
            return Response::builder().status(StatusCode::OK).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
        }
    };

    // check password
    let hasher = Argon2::default();
    let hash = PasswordHash::new(&user.password).unwrap();
    if let Err(err) = hasher.verify_password(password.expose_secret().as_bytes(), &hash) {
        tracing::error!("Failed to verify password: {}", err);
        let html = render_to_string(move || {
            view! {
                <h1> Failed to verify password {err.to_string() } </h1>
            }
        }).into_owned();
        return Response::builder().status(500).body(Body::from(html)).unwrap();
    };

    auth.login_user(user.id);
    // TODO: redirect to user dashboard
    Response::builder().status(StatusCode::FOUND).header("HX-Location", "/dashboard").body(Body::empty()).unwrap()
}

#[tracing::instrument(skip(auth))]
pub async fn login_user_ui(
    auth: AuthSession<User, Uuid, SessionPgPool, PgPool>,
) -> Response<Body> {
    if auth.current_user.is_some() {
        return Response::builder().status(StatusCode::FOUND).header("Location", "/user/token").body(Body::empty()).unwrap();
    }
    let html = render_to_string(|| view! {
        <Base>
            <form 
              hx-post="/login" 
              hx-trigger="submit"
              class="flex flex-col mb-4 gap-1"
            >
                <h1 class="text-2xl font-bold"> Login </h1>
                <label for="username">Username</label>
                <input type="temt" name="username" id="username" required class="input input-bordered w-full max-w-xs" />
                <label for="password">Password</label>
                <input type="password" name="password" id="password" required class="input input-bordered w-full max-w-xs" />
                <button class="mt-4 btn btn-primary w-full max-w-xs">Login</button>
            </form>
        </Base>
    }).into_owned();
    Response::builder().status(StatusCode::OK).header("Content-Type", "text/html").body(Body::from(html)).unwrap()
}
