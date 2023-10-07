use std::{collections::HashSet, fs::File};

use axum::{
    extract::State,
    response::{Html, Response},
    routing::{get, post},
    Form, Router,
};
use axum_session::{SessionConfig, SessionLayer, SessionStore};
use hyper::{Body, StatusCode};
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
use rand::{Rng, SeedableRng};

// Base64 url safe
const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const TOKEN_LENGTH: usize = 32;

use axum_session_auth::*;
use async_trait::async_trait;

use crate::{configuration::Settings, startup::AppState};

pub async fn router(state: AppState, _config: &Settings) -> Router<AppState, Body> {
    let pool = state.pool.clone();
    let session_config = SessionConfig::default();

    let auth_config = AuthConfig::<Uuid>::default().with_anonymous_user_id(Some(Uuid::default()));
        let session_store =
        SessionStore::<SessionPgPool>::new(Some(pool.clone().into()), session_config)
            .await
            .unwrap();


    Router::new()
        .route("/api/user/register", post(register_user_api))
        .route("/user/token", post(create_token))
        .route("/register", get(register_user_ui).post(register_user))
        .route("/login", get(login_user_ui).post(login_user))
        .route("/logout", get(logout_user).post(logout_user))
        .route("/new", get(create_repo_ui).post(create_repo))
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

// TODO: move this out of auth
#[component]
fn base(children: Children) -> impl IntoView{
    view!{
        <html data-theme="night">
            <head>
                <script src="https://unpkg.com/htmx.org@1.9.6"></script>
                // TODO: change tailwind to use node
                <link href="https://cdn.jsdelivr.net/npm/daisyui@3.8.2/dist/full.css" rel="stylesheet" type="text/css" />
                <script src="https://cdn.tailwindcss.com"></script>
            </head>
            <body>
                // need this in body so body exist
                <script> {"
                    document.body.addEventListener('htmx:beforeSwap', function(evt) {{
                      let status = evt.detail.xhr.status;
                      if (status === 500 || status === 422 || status === 400) {{
                        evt.detail.shouldSwap = true;
                        evt.detail.isError = false;
                      }}
                    }});
                "}</script>
                //TODO: maybe make this optional
                <div class="px-8 pt-8 pb-5 flex flex-col sm:px-12 md:px-24 lg:px-28 xl:mx-auto xl:max-w-6xl">
                    {children()}
                </div>
            </body>
        </html>
    }
}

#[derive(Deserialize)]
pub struct UserRequest {
    pub username: String,
    pub name: String,
    pub password: Secret<String>,
}

#[tracing::instrument(skip(auth, pool, password))]
pub async fn register_user(
    auth: AuthSession<User, Uuid, SessionPgPool, PgPool>,
    State(AppState { pool, .. }): State<AppState>,
    Form(UserRequest {
        username,
        name,
        password,
    }): Form<UserRequest>,
) -> Response<Body> {
    // validate username
    // TODO: maybe use rust validator crate
    if username.contains(char::is_whitespace) {
        let html = render_to_string(move || { view! {
            <h1> Username cannot contain whitespace </h1>
        }}).into_owned();
        return Response::builder().status(StatusCode::BAD_REQUEST).header("Content-Type", "text/html").body(Body::from(html)).unwrap();
    }

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
            Response::builder().status(StatusCode::OK).header("Content-Type", "text/html").header("HX-Location", "/new").body(Body::from(html)).unwrap()
        }
    }

}

#[tracing::instrument]
pub async fn register_user_ui() -> Html<String> {
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
    Response::builder().status(StatusCode::FOUND).header("HX-Location", "/new").body(Body::empty()).unwrap()
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



#[derive(Deserialize)]
pub struct TokenRequest {
    pub project_id: String,
}

#[tracing::instrument(skip(auth, pool))]
pub async fn create_token(
    auth: AuthSession<User, Uuid, SessionPgPool, PgPool>,
    State(AppState { pool, .. }): State<AppState>,
    Form(TokenRequest { project_id }): Form<TokenRequest>,
) -> Html<String> {
    if auth.current_user.is_none() {
        return Html(render_to_string(|| { view! {
            <h1> User not authenticated </h1>
        }}).into_owned());
    };

    // check if project id valid UUID
    let project_id = match Uuid::parse_str(&project_id) {
        Ok(id) => id,
        Err(err) => {
            tracing::error!("Failed to parse project id: {}", err);
            return Html(render_to_string(move || { view! {
                <h1> Failed to parse project id {err.to_string() } </h1>
            }}).into_owned());
        }
    };

    // check if project exists
    match sqlx::query!(
        r#"SELECT id FROM projects WHERE id = $1 AND deleted_at IS NULL"#,
        project_id
    ).fetch_one(&pool).await {
        Ok(_) => {},
        Err(sqlx::Error::RowNotFound) => {
            return Html(render_to_string(move || { view! {
                <h1> Project does not exist </h1>
            }}).into_owned());
        },
        Err(err) => {
            tracing::error!("Failed to query database: {}", err);
            return Html(render_to_string(move || { view! {
                <h1> Failed to query database {err.to_string() } </h1>
            }}).into_owned());

        }
    }

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

    let html = render_to_string(move || view! { <p>{token}</p> }).into_owned();
    Html(html)
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Token {
    pub id: Uuid,
    pub name: String,
}


// TODO: we need to finalize the working between repo and project
#[derive(Deserialize)]
pub struct RepoRequest {
    pub owner: String,
    pub project: String,
}

#[tracing::instrument(skip(auth, base, domain))]
pub async fn create_repo(
    auth: AuthSession<User, Uuid, SessionPgPool, PgPool>,
    State(AppState { base, domain, .. }): State<AppState>,
    Form(RepoRequest {
        owner,
        project,
    }): Form<RepoRequest>,
) -> Html<String> {
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


    if File::open(&path).is_ok() {
        return Html(render_to_string(|| { view! {
            <h1> project name already taken </h1>
        }}).into_owned());
    };


    match git2::Repository::init_bare(path) {
        Err(err) => {
            tracing::error!("Failed to create repo: {}", err);
            Html(render_to_string(move || { view! {
                <h1> Failed to query database {err.to_string() } </h1>
            }}).into_owned())
        },
        Ok(_) => 
            Html(render_to_string(move || { view! {
                <h1> Project created successfully  </h1>
                <div class="p-4 bg-gray-800">
                    <pre><code id="code"> 
                        git remote add origin {format!(" http://{domain}/{owner}/{project}")} <br/>
                        {"git push -u origin master"}
                    </code></pre>
                </div>
                <button
                class="btn btn-outline btn-secondary mt-4"
                onclick="
                    let lb = '\\n'
                    if(navigator.userAgent.indexOf('Windows') != -1) {{
                      lb = '\\r\\n'
                    }}

                    let text = document.getElementById('code').getInnerHTML().replaceAll('<br>', lb)
                    console.log(text)
                    if ('clipboard' in window.navigator) {{
                        navigator.clipboard.writeText(text)
                    }}"
                > Copy to clipboard </button>
            }}).into_owned()),
    }
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
pub async fn create_repo_ui(
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

#[tracing::instrument(skip(pool, password))]
pub async fn register_user_api(
    State(AppState { pool, .. }): State<AppState>,
    Form(UserRequest {
        username,
        name,
        password,
    }): Form<UserRequest>,
) -> Response<Body> {

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
