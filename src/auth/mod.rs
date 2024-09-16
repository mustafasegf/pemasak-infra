use std::collections::HashSet;

use axum::{
    middleware::Next,
    response::Response,
};
use axum_session::SessionStore;
use bytes::Bytes;
use http_body::combinators::UnsyncBoxBody;
use hyper::{Body, Request, StatusCode};
use regex::Regex;
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use garde::Validate;

use async_trait::async_trait;
use axum_session_auth::*;

use crate::configuration::Settings;
use lazy_static::lazy_static;

lazy_static! {
    static ref USERNAME_REGEX: Regex = Regex::new(r"^[a-zA-Z0-9.]+$").unwrap();
}

pub mod api;

pub type Auth = AuthSession<User, Uuid, SessionPgPool, PgPool>;

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
        let sqluser = sqlx::query!(
            "SELECT id, username, name, password FROM users WHERE id = $1",
            id
        )
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
    error_type: RegisterUserErrorType,
}
