use std::fmt;

use axum::extract::{Path, State};
use axum::response::Response;
use hyper::{Body, StatusCode};
use leptos::ssr::render_to_string;
use leptos::{view, IntoView};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::components::Base;
use crate::projects::components::ProjectHeader;
use crate::{auth::Auth, startup::AppState};

#[derive(Serialize, Deserialize, Debug, sqlx::Type)]
#[sqlx(type_name = "build_state", rename_all = "lowercase")]
pub enum BuildState {
    PENDING,
    BUILDING,
    SUCCESSFUL,
    FAILED,
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

#[tracing::instrument(skip(auth, pool))]
pub async fn get(
    auth: Auth,
    State(AppState {
        pool,
        domain,
        secure,
        ..
    }): State<AppState>,
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
            let html = render_to_string(move || {
                view! {
                    <Base is_logged_in={true}>
                        <h1> Project does not exist </h1>
                    </Base>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(html))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get projects: Failed to query database");
            let html = render_to_string(move || {
                view! {
                    <h1> "Failed to query database " {err.to_string() } </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(html))
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
            let html = render_to_string(move || {
                view! {
                    <h1> "Failed to query database " {err.to_string() } </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(html))
                .unwrap();
        }
    };

    let html = render_to_string(move || {
        view! {
            <Base is_logged_in={true}>
              <ProjectHeader owner={owner.clone()} project={project.clone()} domain={domain.clone()}></ProjectHeader>

              <h2 class="text-xl">
                "Build Log - ID: "{build.id.to_string()}
              </h2>
              <div class="w-full mt-4 px-1 mockup-code bg-neutral/40 backdrop-blur-sm">
                {build.log.split('\n').map(|line| { view!{
                    <pre><code>{line.to_string()}</code></pre>
                }}).collect::<Vec<_>>()}
              </div>
            </Base>
        }
    })
    .into_owned();

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(html))
        .unwrap()
}
