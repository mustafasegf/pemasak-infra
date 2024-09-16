use axum::{
    extract::{Json, State},
    response::Response,
};
use hyper::{Body, StatusCode};
use secrecy::ExposeSecret;
use serde::{Serialize, Deserialize};
use ulid::Ulid;
use uuid::Uuid;

use garde::Unvalidated;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};

use crate::{
    auth::{Auth, ErrorResponse, RegisterUserErrorType, UserRequest},
    startup::AppState,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum SsoResponse {
    #[serde(rename_all = "camelCase")]
    ServiceResponse {
        service_response: ServiceResponse,
    },
    Error {
        error: String,
    },
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
                .body(Body::from(
                    serde_json::to_string(&ErrorResponse {
                        message: err.to_string(),
                        error_type: RegisterUserErrorType::ValidationError,
                    })
                    .unwrap(),
                ))
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
            })
            .unwrap();

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
            })
            .unwrap();
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
            })
            .unwrap();

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
            })
            .unwrap();
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
            })
            .unwrap();

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
            })
            .unwrap();

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
            .body(
                serde_json::json!({
                    "username": username,
                    "password": password.expose_secret(),
                    "casUrl": "https://sso.ui.ac.id/cas/",
                    "serviceUrl": "http%3A%2F%2Fberanda.ui.ac.id%2Fpersonal%2F",
                    "EncodeUrl": true
                })
                .to_string(),
            )
            .send()
            .await
        {
            Ok(res) => res,
            Err(err) => {
                tracing::error!(?err, "Can't register user: Failed to request sso");
                if let Err(err) = tx.rollback().await {
                    tracing::error!(?err, "Can't register user: Failed to rollback transaction");
                }

                let json = serde_json::to_string(&ErrorResponse {
                    message: format!("failed to request sso: {}", err.to_string()),
                    error_type: RegisterUserErrorType::InternalServerError,
                })
                .unwrap();

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
                })
                .unwrap();

                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header("Content-Type", "text/html")
                    .body(Body::from(json))
                    .unwrap();
            }
        };

        tracing::warn!(?body);

        let sso_res = match serde_json::from_slice::<SsoResponse>(&body) {
            Ok(SsoResponse::ServiceResponse { service_response }) => {
                service_response.authentication_success.attributes
            }
            Ok(SsoResponse::Error { .. }) => {
                let json = serde_json::to_string(&ErrorResponse {
                    message: "Wrong username or password".to_string(),
                    error_type: RegisterUserErrorType::SSOError,
                })
                .unwrap();

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
                })
                .unwrap();

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
            })
            .unwrap();

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
        })
        .unwrap();

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
        })
        .unwrap();
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
        })
        .unwrap();

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
            })
            .unwrap();
            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/html")
                .body(Body::from(json))
                .unwrap()
        }
        Ok(_) => {
            auth.login_user(user_id);
            let json = serde_json::to_string(&RegisterUserSuccessResponse {
                message: "User Created".to_string(),
            })
            .unwrap();
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/html")
                .header("HX-Location", "/api/dashboard")
                .body(Body::from(json))
                .unwrap()
        }
    }
}
