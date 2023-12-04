use axum::extract::{Path, State};
use axum::response::Response;
use hyper::{Body, StatusCode};
use leptos::ssr::render_to_string;
use leptos::{view, IntoView};

use crate::components::Base;
use crate::projects::components::ProjectHeader;
use crate::{auth::Auth, startup::AppState};

#[tracing::instrument(skip(auth, pool))]
pub async fn get(
    auth: Auth,
    State(AppState { pool, domain, .. }): State<AppState>,
    Path((owner, project)): Path<(String, String)>,
) -> Response<Body> {
    let _user = auth.current_user.unwrap();

    let delete_path = format!("/{owner}/{project}/delete");
    let volume_path = format!("/{owner}/{project}/volume/delete");

    // check if project exist
    let _project = match sqlx::query!(
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

    let html = render_to_string(move || {
        view! {
            <Base is_logged_in={true}>
              <ProjectHeader owner={owner} project={project} domain={domain}></ProjectHeader>

              <h2 class="text-xl">
                Project Controls
              </h2>
              <div class="flex w-full space-x-4 items-center mb-8">
                <button
                  hx-post={delete_path}
                  hx-trigger="click"
                  class="btn btn-error mt-4 w-full max-w-xs"
                >Delete Project</button>

                <button
                  hx-post={volume_path}
                  hx-trigger="click"
                  class="btn btn-error mt-4 w-full max-w-xs"
                >Delete Database</button>
              </div>

              <div id="result"></div>
            </Base>
        }
    })
    .into_owned();

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(html))
        .unwrap()
}
