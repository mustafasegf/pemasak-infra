use std::fmt;

use axum::extract::{State, Path};
use axum::response::Response;
use chrono::{DateTime, Utc};
use hyper::{Body, StatusCode};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

use crate::{auth::Auth, startup::AppState};

#[derive(Serialize, Deserialize, Debug, sqlx::Type)]
#[sqlx(type_name = "build_state", rename_all = "lowercase")] 
pub enum BuildState {
    PENDING,
    BUILDING,
    SUCCESSFUL,
    FAILED
}

impl fmt::Display for BuildState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BuildState::PENDING => write!(f, "Pending"),
            BuildState::BUILDING => write!(f, "Building"),
            BuildState::SUCCESSFUL => write!(f, "Successful"),
            BuildState::FAILED => write!(f, "Failed"),
        }
    }
}

#[derive(Serialize, Debug)]
struct BuildDetailResponse {
    id: Uuid,
    status: BuildState,
    created_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    logs: String
}

#[derive(Serialize, Debug)]
struct ErrorResponse {
    message: String,
}

#[tracing::instrument(skip(auth, pool))]
pub async fn get(
    auth: Auth,
    State(AppState { pool, domain, secure, .. }): State<AppState>,
    Path((owner, project, build_id)): Path<(String, String, Uuid)>,
) -> Response<Body> {
    let _user = auth.current_user.unwrap();

    // check if project exist
    let _project_record = match sqlx::query!(
        r#"SELECT projects.id, projects.name AS project, project_owners.name AS owner
           FROM projects
           JOIN project_owners ON projects.owner_id = project_owners.id
           JOIN users_owners ON project_owners.id = users_owners.owner_id
           AND projects.name = $1
           AND project_owners.name = $2
        "#,
        project,
        owner,
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(record)) => record,
        Ok(None) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: "Project does not exist".to_string()
            }).unwrap();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(json))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get projects: Failed to query database");

            let json = serde_json::to_string(&ErrorResponse {
                message: format!("Failed to query database: {}", err.to_string())
            }).unwrap();

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(json))
                .unwrap();
        }
    };

    let build = match sqlx::query!(
        r#"SELECT id, project_id, status AS "status: BuildState", created_at, finished_at, log 
        FROM builds WHERE id = $1
        ORDER BY created_at DESC"#,
        build_id
    )
    .fetch_one(&pool)
    .await 
    {
        Ok(record) => record,
        Err(err) => {
            let json = serde_json::to_string(&ErrorResponse {
                message: format!("Failed to query database: {}", err.to_string())
            }).unwrap();

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(json))
                .unwrap();
        }, 
    };

    let json = serde_json::to_string(&BuildDetailResponse {
        id: build.id,
        status: build.status,
        created_at: build.created_at,
        finished_at: build.finished_at,
        logs: build.log,
    }).unwrap();

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(json))
        .unwrap()
}
