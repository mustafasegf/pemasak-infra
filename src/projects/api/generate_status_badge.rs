use std::fmt;

use axum::extract::{State, Path};
use axum::response::Response;
use hyper::{Body, StatusCode};
use serde::{Serialize, Deserialize};

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
struct ErrorResponse {
    message: String,
}

#[tracing::instrument(skip(auth, pool))]
pub async fn get(
    auth: Auth,
    State(AppState { pool, domain, secure, .. }): State<AppState>,
    Path((owner, project)): Path<(String, String)>,
) -> Response<Body> {
    let _user = auth.current_user.unwrap();

    // check if project exist
    let project_record = match sqlx::query!(
        r#"SELECT projects.id
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
        FROM builds WHERE project_id = $1
        ORDER BY created_at DESC"#,
        project_record.id
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

    let mut style = badgen::Style::flat();
    
    style.background = match &build.status {
        BuildState::PENDING => badgen::Color::Grey,
        BuildState::FAILED => badgen::Color::Red,
        BuildState::SUCCESSFUL => badgen::Color::Green,
        BuildState::BUILDING => badgen::Color::Yellow,
    };

    let badge = badgen::badge(
        &style, 
        &build.status.to_string(),
        Some("PWS Build Status"), 
    ).unwrap();

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "image/svg+xml")
        .body(Body::from(badge))
        .unwrap()
}
