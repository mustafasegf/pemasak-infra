use std::collections::HashSet;

use axum::{
    extract::State,
    middleware::Next,
    response::{Html, Response},
    routing::get,
    Form, Router,
};
use axum_extra::routing::RouterExt;
use axum_session::SessionStore;
use bytes::Bytes;
use http_body::combinators::UnsyncBoxBody;
use hyper::{Body, Request, StatusCode};
use leptos::{ssr::render_to_string, *};
use regex::Regex;
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use ulid::Ulid;
use uuid::Uuid;

use garde::{Unvalidated, Validate};

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

use async_trait::async_trait;
use axum_session_auth::*;

use crate::{components::Base, configuration::Settings, startup::AppState};
use lazy_static::lazy_static;

lazy_static! {
    static ref USERNAME_REGEX: Regex = Regex::new(r"^[a-zA-Z0-9.]+$").unwrap();
}

pub type Auth = AuthSession<User, Uuid, SessionPgPool, PgPool>;

pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route_with_tsr("/register", get(register_user_ui).post(register_user))
        .route_with_tsr("/login", get(login_user_ui).post(login_user))
        .route_with_tsr("/logout", get(logout_user).post(logout_user))
}

pub async fn auth<B>(
    auth: Auth,
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

pub async fn auth_layer(
    pool: &PgPool,
    config: &Settings,
) -> (AuthConfig<Uuid>, SessionStore<SessionPgPool>) {
    let session_config = config.session_config();
    let auth_config = AuthConfig::<Uuid>::default();

    let session_store =
        SessionStore::<SessionPgPool>::new(Some(pool.clone().into()), session_config)
            .await
            .unwrap();
    (auth_config, session_store)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password: String,
    pub permissions: HashSet<String>,
}

// TODO: do we need this?
impl User {
    pub async fn get(id: &Uuid, pool: &PgPool) -> Result<User, sqlx::Error> {
        let sqluser = sqlx::query!("SELECT id, username, password FROM users WHERE id = $1", id)
            .fetch_one(pool)
            .await?;

        let sql_user_perms =
            sqlx::query!("SELECT token FROM user_permissions WHERE user_id = $1;", id)
                .fetch_all(pool)
                .await?;

        Ok(Self {
            id: sqluser.id,
            username: sqluser.username,
            password: sqluser.password,
            permissions: sql_user_perms.into_iter().map(|x| x.token).collect(),
        })
    }

    pub async fn get_from_username(username: &str, pool: &PgPool) -> Result<Self, sqlx::Error> {
        let sqluser = sqlx::query!(
            "SELECT id, username, password FROM users WHERE username = $1",
            username
        )
        .fetch_one(pool)
        .await?;

        let sql_user_perms = sqlx::query!(
            "SELECT token FROM user_permissions WHERE user_id = $1;",
            sqluser.id
        )
        .fetch_all(pool)
        .await?;

        Ok(Self {
            id: sqluser.id,
            username: sqluser.username,
            password: sqluser.password,
            permissions: sql_user_perms.into_iter().map(|x| x.token).collect(),
        })
    }
}

#[async_trait]
impl Authentication<User, Uuid, PgPool> for User {
    async fn load_user(id: Uuid, pool: Option<&PgPool>) -> Result<User, anyhow::Error> {
        let pool = pool.unwrap();

        User::get(&id, pool).await.map_err(|err| {
            tracing::error!(?err, "Can't get user: Failed to query database");
            anyhow::Error::new(err)
        })
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

fn password_check(value: &Secret<String>, _ctx: &()) -> garde::Result {
    if value.expose_secret().is_empty() {
        return Err(garde::Error::new("Password cannot be empty"));
    }
    Ok(())
}

// why we use this and not the default garde regex match is becasue we need better error code until
// https://github.com/jprochazk/garde/issues/7 is merged
fn username_check(value: &str, _ctx: &()) -> garde::Result {
    if !USERNAME_REGEX.is_match(value) {
        return Err(garde::Error::new(
            "Username can only contain alphanumeric characters and dots",
        ));
    }
    Ok(())
}

#[derive(Deserialize, Validate, Debug)]
pub struct UserRequest {
    #[garde(custom(username_check))]
    pub username: String,
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(custom(password_check))]
    pub password: Secret<String>,
}

// auto gen


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum SsoResponse {
    #[serde(rename_all = "camelCase")]
    ServiceResponse { service_response: ServiceResponse },
    Error { error: String },
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceResponse {
    pub authentication_success: AuthenticationSuccess,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticationSuccess {
    pub attributes: Attributes,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attributes {
    pub jurusan: Jurusan,
    #[serde(rename = "ldap_role")]
    pub ldap_role: String,
    #[serde(rename = "status_mahasiswa")]
    pub status_mahasiswa: String,
    #[serde(rename = "status_mahasiswa_aktif")]
    pub status_mahasiswa_aktif: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Jurusan {
    pub faculty: String,
    pub short_faculty: String,
    pub major: String,
    pub program: String,
}


#[tracing::instrument(skip(auth, pool))]
pub async fn register_user(
    auth: Auth,
    State(AppState { pool, sso, .. }): State<AppState>,
    Form(req): Form<Unvalidated<UserRequest>>,
) -> Response<Body> {
    let UserRequest {
        username,
        name,
        password,
    } = match req.validate(&()) {
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

    // check if user exists
    match sqlx::query!("SELECT username FROM users WHERE username = $1", username)
        .fetch_optional(&pool)
        .await
    {
        Ok(None) => {}
        Err(err) => {
            tracing::error!(?err, "Can't get user: Failed to query database");
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to query database {err.to_string() } </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }

        Ok(_) => {
            let html = render_to_string(|| {
                view! {
                    <h1> Username already exists </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }
    }

    // check if owner exists
    match sqlx::query!(
        r#"SELECT name FROM project_owners WHERE name = $1"#,
        username
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(None) => {}
        Err(err) => {
            tracing::error!(?err, "Can't get owners: Failed to query database");
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to query database {err.to_string() } </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }

        Ok(_) => {
            let html = render_to_string(|| {
                view! {
                    <h1> Username already exists </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }
    }

    let user_id = Uuid::from(Ulid::new());
    let hasher = Argon2::default();
    let salt = SaltString::generate(&mut OsRng);

    let password_hash = match hasher.hash_password(password.expose_secret().as_bytes(), &salt) {
        Ok(hash) => hash,
        Err(err) => {
            tracing::error!(?err, "Can't register User: Failed to hash password");
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to hash password {err.to_string() } </h1>
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

    // TODO: use actual sso and not proxy
    if sso {

        // TODO: not sure if this is the best way to do this
        let client = reqwest::Client::new();
        let res = match client
            .post("https://sso.mus.sh")
            .body(serde_json::json!({
                "username": username,
                "password": password.expose_secret(),
                "casUrl": "https://sso.ui.ac.id/cas/",
                "serviceUrl": "http%3A%2F%2Fberanda.ui.ac.id%2Fpersonal%2F",
                "EncodeUrl": true
            }).to_string())
            .send()
            .await {
            Ok(res) => res,
            Err(err) => {
                tracing::error!(?err, "Can't register user: Failed to request sso");
                if let Err(err) = tx.rollback().await {
                    tracing::error!(?err, "Can't register user: Failed to rollback transaction");
                }

                let html = render_to_string(move || {
                    view! {
                        <h1> Failed to request sso {err.to_string() } </h1>
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

        let body = match res.bytes().await {
            Ok(body) => body,
            Err(err) => {
                tracing::error!(?err, "Can't register user: Failed to get body");
                if let Err(err) = tx.rollback().await {
                    tracing::error!(?err, "Can't register user: Failed to rollback transaction");
                }

                let html = render_to_string(move || {
                    view! {
                        <h1> Failed to get body {err.to_string() } </h1>
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

        tracing::warn!(?body);

        let sso_res = match serde_json::from_slice::<SsoResponse>(&body) {
            Ok(SsoResponse::ServiceResponse { service_response }) => service_response.authentication_success.attributes,
            Ok(SsoResponse::Error { .. }) => {
                let html = render_to_string(move || {
                    view! {
                        <h1> "Wrong username or password" </h1>
                    }
                })
                .into_owned();

                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header("Content-Type", "text/html")
                    .body(Body::from(html))
                    .unwrap();
            }
            Err(err) => {
                tracing::error!(?err, "Can't register user: Failed to parse body");
                if let Err(err) = tx.rollback().await {
                    tracing::error!(?err, "Can't register user: Failed to rollback transaction");
                }

                let html = render_to_string(move || {
                    view! {
                        <h1> Failed to parse body {err.to_string() } </h1>
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

        if sso_res.jurusan.faculty != "Ilmu Komputer" {
            let html = render_to_string(move || {
                view! {
                    <h1> User is not from Ilmu Komputer </h1>
                }
            })
            .into_owned();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }
    }


    if let Err(err) = sqlx::query!(
        r#"INSERT INTO users (id, username, password, name) VALUES ($1, $2, $3, $4)"#,
        user_id,
        username,
        password_hash.to_string(),
        name
    )
    .execute(&mut *tx)
    .await
    {
        tracing::error!(?err, "Can't insert user: Failed to insert into database");
        if let Err(err) = tx.rollback().await {
            tracing::error!(?err, "Can't insert user: Failed to rollback transaction");
        }

        let html = render_to_string(move || {
            view! {
                <h1> Failed to insert into database </h1>
            }
        })
        .into_owned();

        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(Body::from(html))
            .unwrap();
    };

    let owner_id = Uuid::from(Ulid::new());

    if let Err(err) = sqlx::query!(
        r#"INSERT INTO project_owners (id, name) VALUES ($1, $2)"#,
        owner_id,
        username
    )
    .execute(&mut *tx)
    .await
    {
        tracing::error!(
            ?err,
            "Can't insert project_owners: Failed to insert into database"
        );
        if let Err(err) = tx.rollback().await {
            tracing::error!(
                ?err,
                "Can't insert project_owners: Failed to rollback transaction"
            );
        }

        let html = render_to_string(move || {
            view! {
                <h1>Failed to insert into database</h1>
            }
        })
        .into_owned();
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(Body::from(html))
            .unwrap();
    };

    if let Err(err) = sqlx::query!(
        r#"INSERT INTO users_owners (user_id, owner_id) VALUES ($1, $2)"#,
        user_id,
        owner_id,
    )
    .execute(&mut *tx)
    .await
    {
        tracing::error!(
            ?err,
            "Can't insert users_owners: Failed to insert into database"
        );

        if let Err(err) = tx.rollback().await {
            tracing::error!(
                ?err,
                "Can't insert users_owners: Failed to rollback transaction"
            );
        }
        let html = render_to_string(move || {
            view! {
                <h1> Failed to insert into database </h1>
            }
        })
        .into_owned();

        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(Body::from(html))
            .unwrap();
    }

    match tx.commit().await {
        Err(err) => {
            tracing::error!(?err, "Can't register user: Failed to commit transaction");
            let html = render_to_string(move || {
                view! {
                    <h1> Failed to commit transaction {err.to_string() } </h1>
                }
            })
            .into_owned();
            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap()
        }
        Ok(_) => {
            auth.login_user(user_id);
            let html = render_to_string(|| {
                view! {
                    <h1> User created </h1>
                }
            })
            .into_owned();
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/html")
                .header("HX-Location", "/dashboard")
                .body(Body::from(html))
                .unwrap()
        }
    }
}

#[tracing::instrument]
pub async fn register_user_ui(
    auth: Auth,
    State(AppState { build_channel, .. }): State<AppState>,
) -> Html<String> {
    let is_logged_in: bool = auth.current_user.is_some();

    let html = render_to_string(move || {
        view! {
            <Base is_logged_in={is_logged_in} class={"!pt-0".to_string()}>
                <form 
                    class="flex flex-col p-12 gap-8 w-full md:w-1/2 mx-auto my-auto bg-slate-900/30 rounded-lg backdrop-blur-sm border border-1 border-slate-700"
                    hx-post="/register" 
                    hx-trigger="submit"
                    hx-target="#result"
                    >
                    <h1 class="w-full text-center text-3xl font-bold"> Register Account </h1>

                    <div class="flex flex-col gap-4">
                        <div class="flex flex-col gap-2">
                            <label for="username">"Username (SSO UI)"</label>
                            <input type="text" name="username" id="username" required class="input input-bordered w-full" />
                        </div>
                        
                        <div class="flex flex-col gap-2">
                            <label for="password">"Password (SSO UI)"</label>
                            <input type="password" name="password" id="password" required class="input input-bordered w-full" />
                        </div>

                        <div class="flex flex-col gap-2">
                            <label for="name">Full Name</label>
                            <input type="text" name="name" id="name" required class="input input-bordered w-full" />
                        </div>
                    </div>
                    
                    <button class="mt-4 btn btn-primary w-full">Register</button>
                </form>

                <div id="result"></div>
            </Base>
        }
    })
    .into_owned();

    Html(html)
}

#[tracing::instrument(skip(auth))]
pub async fn logout_user(auth: Auth) -> Response<Body> {
    auth.logout_user();
    Response::builder()
        .status(StatusCode::FOUND)
        .header("Location", "/login")
        .body(Body::empty())
        .unwrap()
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: Secret<String>,
}

#[tracing::instrument(skip(auth, pool, password))]
pub async fn login_user(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
    Form(LoginRequest { username, password }): Form<LoginRequest>,
) -> Response<Body> {
    // get user
    let user = match User::get_from_username(&username, &pool).await {
        Ok(user) => user,
        Err(_err) => {
            let html = render_to_string(|| {
                view! {
                    <h1> User does not exist </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/html")
                .body(Body::from(html))
                .unwrap();
        }
    };

    // check password
    let hasher = Argon2::default();
    let hash = PasswordHash::new(&user.password).unwrap();
    if let Err(err) = hasher.verify_password(password.expose_secret().as_bytes(), &hash) {
        tracing::error!(?err, "Can't login: Failed to verify password");
        let html = render_to_string(move || {
            view! {
                <h1> Failed to verify password {err.to_string() } </h1>
            }
        })
        .into_owned();
        return Response::builder()
            .status(500)
            .body(Body::from(html))
            .unwrap();
    };

    auth.login_user(user.id);
    Response::builder()
        .status(StatusCode::FOUND)
        .header("HX-Location", "/dashboard")
        .body(Body::empty())
        .unwrap()
}

#[tracing::instrument(skip(auth))]
pub async fn login_user_ui(auth: Auth) -> Response<Body> {
    if auth.current_user.is_some() {
        return Response::builder()
            .status(StatusCode::FOUND)
            .header("Location", "/dashboard")
            .body(Body::empty())
            .unwrap();
    }

    let is_logged_in = auth.current_user.is_some();

    let html = render_to_string(move || view! {
        <Base is_logged_in={is_logged_in} class={"!pt-0".to_string()}>
            <form 
                class="flex flex-col p-12 gap-8 w-full md:w-1/2 mx-auto my-auto bg-slate-900/30 rounded-lg backdrop-blur-sm border border-1 border-slate-700"
                hx-post="/login" 
                hx-trigger="submit"
                >
                <h1 class="w-full text-center text-3xl font-bold"> Login </h1>

                <div class="flex flex-col gap-4">
                    <div class="flex flex-col gap-2">
                    <label for="username">Username</label>
                    <input type="temt" name="username" id="username" required class="input input-bordered w-full" />
                    </div>

                    <div class="flex flex-col gap-2">
                    <label for="password">Password</label>
                    <input type="password" name="password" id="password" required class="input input-bordered w-full" />
                    </div>
                </div>
                
                <button class="btn btn-primary w-full">Login</button>
                <p class="text-center">
                    {"Don't have an account? "}<br /><a class="hover:underline text-secondary" href="/register">Register Here</a>
                </p>
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
