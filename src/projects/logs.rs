use axum::extract::{Path, State};
use axum::response::Response;
use bollard::Docker;
use bytes::Bytes;
use futures_util::TryStreamExt;
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

    // check if project exist
    let _project_id = match sqlx::query!(
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
        Ok(Some(record)) => record.id,
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

    let docker = match Docker::connect_with_local_defaults() {
        Err(err) => {
            tracing::error!(?err, "Can't delete project: Failed to connect to docker");
            let html = render_to_string(move || {
                view! {
                    <h1> "Failed to connect to docker" </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(html))
                .unwrap();
        }
        Ok(docker) => docker,
    };

    let container_name = format!("{owner}-{}", project.trim_end_matches(".git")).replace('.', "-");

    let logs = docker
        .logs(
            &container_name,
            Some(bollard::container::LogsOptions::<&str> {
                stdout: true,
                stderr: true,
                ..Default::default()
            }),
        )
        .try_collect::<Vec<_>>()
        .await
        .unwrap()
        .into_iter()
        .map(|log_output| {
            let b = match log_output {
                bollard::container::LogOutput::StdOut { message } => message,
                bollard::container::LogOutput::StdErr { message } => message,
                // bollard::container::LogOutput::StdIn { message } => message,
                _ => Bytes::new(),
            };
            String::from_utf8(b.to_vec()).unwrap()
        })
        .collect::<Vec<_>>();
        // .join("");

    // TODO: add env modification
    let html = render_to_string(move || {
        view! {
            <Base is_logged_in={true}>
              <ProjectHeader owner={owner.clone()} project={project.clone()} domain={domain.clone()}></ProjectHeader>

              <h2 class="text-xl">
                "Logs"
              </h2>
              <div class="w-full mt-4 px-1 mockup-code bg-neutral/40 backdrop-blur-sm">
                {logs.into_iter().map(|log| { view!{
                    <pre><code>{log}</code></pre>
                }}).collect::<Vec<_>>()}
                // <pre><code>{log}</code></pre>
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
