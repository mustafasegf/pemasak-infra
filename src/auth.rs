use std::collections::HashSet;

use axum::{
    extract::{Json, State}, middleware::Next, response::Response, routing::{get, post}, Router
};
use axum_extra::routing::RouterExt;
use axum_session::SessionStore;
use bytes::Bytes;
use http_body::combinators::UnsyncBoxBody;
use hyper::{Body, Request, StatusCode};
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

use crate::{configuration::Settings, startup::AppState};
use lazy_static::lazy_static;

lazy_static! {
    static ref USERNAME_REGEX: Regex = Regex::new(r"^[a-zA-Z0-9.]+$").unwrap();
}

pub type Auth = AuthSession<User, Uuid, SessionPgPool, PgPool>;

pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route_with_tsr("/api/register", post(register_user))
        .route_with_tsr("/api/login", post(login_user))
        .route_with_tsr("/api/logout", get(logout_user).post(logout_user))
        .route_with_tsr("/api/validate", get(validate_auth))
}

pub async fn auth<B>(
    auth: Auth,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response<UnsyncBoxBody<Bytes, axum::Error>>, hyper::Response<Body>> {
    if auth.current_user.is_none() {
        return Err(Response::builder()
            .status(StatusCode::FOUND)
            .header("Location", "/api/login")
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
    pub name: String,
    pub permissions: HashSet<String>,
}

// TODO: do we need this?
impl User {
    pub async fn get(id: &Uuid, pool: &PgPool) -> Result<User, sqlx::Error> {
        let sqluser = sqlx::query!("SELECT id, username, name, password FROM users WHERE id = $1", id)
            .fetch_one(pool)
            .await?;

        let sql_user_perms =
            sqlx::query!("SELECT token FROM user_permissions WHERE user_id = $1;", id)
                .fetch_all(pool)
                .await?;

        Ok(Self {
            id: sqluser.id,
            username: sqluser.username,
            name: sqluser.name,
            password: sqluser.password,
            permissions: sql_user_perms.into_iter().map(|x| x.token).collect(),
        })
    }

    pub async fn get_from_username(username: &str, pool: &PgPool) -> Result<Self, sqlx::Error> {
        let sqluser = sqlx::query!(
            "SELECT id, username, name, password FROM users WHERE username = $1",
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
            name: sqluser.name,
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

#[derive(Serialize, Debug)]
enum RegisterUserErrorType {
    ValidationError,
    BadRequestError,
    InternalServerError,
    SSOError,
}

#[derive(Serialize, Debug)]
struct ErrorResponse {
    message: String,
    error_type: RegisterUserErrorType
}

#[derive(Serialize, Debug)]
struct RegisterUserSuccessResponse {
    message: String,
}

#[tracing::instrument(skip(auth, pool))]
pub async fn register_user(
    auth: Auth,
    State(AppState { pool, sso, .. }): State<AppState>,
    Json(req): Json<Unvalidated<UserRequest>>,
) -> Response<Body> {
    let UserRequest {
        username,
        name,
        password,
    } = match req.validate(&()) {
        Ok(valid) => valid.into_inner(),
        Err(err) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(
                    Body::from(
                        serde_json::to_string(
                            &ErrorResponse {
                                message: err.to_string(),
                                error_type: RegisterUserErrorType::ValidationError,
                            }
                        ).unwrap()
                    )
                )
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
            let json = serde_json::to_string(&ErrorResponse {
                message: format!("failed to query database: {}", err.to_string()),
                error_type: RegisterUserErrorType::InternalServerError,
            }).unwrap();
            
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
                .unwrap();
        }

        Ok(_) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: "Username already exists".to_string(),
                error_type: RegisterUserErrorType::BadRequestError,
            }).unwrap();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
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
            let json = serde_json::to_string(&ErrorResponse {
                message: format!("failed to query database: {}", err.to_string()),
                error_type: RegisterUserErrorType::InternalServerError,
            }).unwrap();
            
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
                .unwrap();
        }

        Ok(_) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: "Username already exists".to_string(),
                error_type: RegisterUserErrorType::BadRequestError,
            }).unwrap();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
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
            let json = serde_json::to_string(&ErrorResponse {
                message: format!("failed to hash password: {}", err.to_string()),
                error_type: RegisterUserErrorType::InternalServerError,
            }).unwrap();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
                .unwrap();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            tracing::error!(?err, "Can't insert user: Failed to begin transaction");
            let json = serde_json::to_string(&ErrorResponse {
                message: "failed to request sso: Failed to begin transaction".to_string(),
                error_type: RegisterUserErrorType::InternalServerError,
            }).unwrap();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
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

                let json = serde_json::to_string(&ErrorResponse {
                    message: format!("failed to request sso: {}", err.to_string()),
                    error_type: RegisterUserErrorType::InternalServerError,
                }).unwrap();

                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header("Content-Type", "text/html")
                    .body(Body::from(json))
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

                let json = serde_json::to_string(&ErrorResponse {
                    message: format!("failed to get body: {}", err.to_string()),
                    error_type: RegisterUserErrorType::SSOError,
                }).unwrap();

                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header("Content-Type", "text/html")
                    .body(Body::from(json))
                    .unwrap();
            }
        };

        tracing::warn!(?body);

        let sso_res = match serde_json::from_slice::<SsoResponse>(&body) {
            Ok(SsoResponse::ServiceResponse { service_response }) => service_response.authentication_success.attributes,
            Ok(SsoResponse::Error { .. }) => {
                let json = serde_json::to_string(&ErrorResponse {
                    message: "Wrong username or password".to_string(),
                    error_type: RegisterUserErrorType::SSOError,
                }).unwrap();

                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header("Content-Type", "text/html")
                    .body(Body::from(json))
                    .unwrap();
            }
            Err(err) => {
                tracing::error!(?err, "Can't register user: Failed to parse body");
                if let Err(err) = tx.rollback().await {
                    tracing::error!(?err, "Can't register user: Failed to rollback transaction");
                }

                let json = serde_json::to_string(&ErrorResponse {
                    message: format!("failed to parse body: {}", err.to_string()),
                    error_type: RegisterUserErrorType::SSOError,
                }).unwrap();

                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header("Content-Type", "text/html")
                    .body(Body::from(json))
                    .unwrap();
            }
        };

        if sso_res.jurusan.faculty != "Ilmu Komputer" {
            let json = serde_json::to_string(&ErrorResponse {
                message: "User is not from UI Faculty of Computer Science".to_string(),
                error_type: RegisterUserErrorType::SSOError,
            }).unwrap();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
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

        let json = serde_json::to_string(&ErrorResponse {
            message: format!("failed to insert into database: {}", err.to_string()),
            error_type: RegisterUserErrorType::InternalServerError,
        }).unwrap();

        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(Body::from(json))
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

        let json = serde_json::to_string(&ErrorResponse {
            message: format!("failed to insert into database: {}", err.to_string()),
            error_type: RegisterUserErrorType::InternalServerError,
        }).unwrap();
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(Body::from(json))
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
        let json = serde_json::to_string(&ErrorResponse {
            message: format!("failed to insert into database: {}", err.to_string()),
            error_type: RegisterUserErrorType::InternalServerError,
        }).unwrap();

        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(Body::from(json))
            .unwrap();
    }

    match tx.commit().await {
        Err(err) => {
            tracing::error!(?err, "Can't register user: Failed to commit transaction");
            let json = serde_json::to_string(&ErrorResponse {
                message: format!("failed to commit transaction: {}", err.to_string()),
                error_type: RegisterUserErrorType::InternalServerError,
            }).unwrap();
            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
                .unwrap()
        }
        Ok(_) => {
            auth.login_user(user_id);
            let json = serde_json::to_string(&RegisterUserSuccessResponse {
                message: "User Created".to_string()
            }).unwrap();
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/html")
                .header("HX-Location", "/api/dashboard")
                .body(Body::from(json))
                .unwrap()
        }
    }
}

#[tracing::instrument(skip(auth))]
pub async fn logout_user(auth: Auth) -> Response<Body> {
    auth.logout_user();
    Response::builder()
        .status(StatusCode::FOUND)
        .header("Location", "/api/login")
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
    Json(LoginRequest { username, password }): Json<LoginRequest>,
) -> Response<Body> {
    // get user
    let user = match User::get_from_username(&username, &pool).await {
        Ok(user) => user,
        Err(_err) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: "Wrong username or password entered".to_string(),
                error_type: RegisterUserErrorType::BadRequestError,
            }).unwrap();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
                .unwrap();
        }
    };

    // check password
    let hasher = Argon2::default();
    let hash = PasswordHash::new(&user.password).unwrap();
    if let Err(err) = hasher.verify_password(password.expose_secret().as_bytes(), &hash) {
        tracing::error!(?err, "Can't login: Failed to verify password");
        let json = serde_json::to_string(&ErrorResponse {
            message: "Wrong username or password entered".to_string(),
            error_type: RegisterUserErrorType::BadRequestError,
        }).unwrap();
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(json))
            .unwrap();
    };

    auth.login_user(user.id);
    Response::builder()
        .status(StatusCode::FOUND)
        .header("HX-Location", "/api/dashboard")
        .body(Body::empty())
        .unwrap()
}

#[derive(Serialize, Debug)]
pub struct ValidateAuthResponse {
    id: Uuid,
    username: String,
    name: String,
}

#[tracing::instrument(skip(auth))]
pub async fn validate_auth(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
) -> Response<Body> {
    if auth.current_user.is_none() {
        return Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::empty())
            .unwrap()
    }

    let current_user = auth.current_user.unwrap();
    let user = match User::get_from_username(&current_user.username, &pool).await {
        Ok(user) => user,
        Err(_err) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: "User not found".to_string(),
                error_type: RegisterUserErrorType::BadRequestError,
            }).unwrap();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
                .unwrap();
        }
    };

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(
            serde_json::to_string(
                &ValidateAuthResponse {
                    id: user.id,
                    username: user.username,
                    name: user.name,
                }
            ).unwrap()
        ))
        .unwrap()
}