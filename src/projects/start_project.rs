use axum::extract::{Path, State};
use axum::response::Response;
use bollard::container::StartContainerOptions;
use bollard::Docker;
use hyper::{Body, StatusCode};
use leptos::ssr::render_to_string;
use leptos::{view, IntoView};

use crate::startup::AppState;

#[tracing::instrument(skip(pool))]
pub async fn post(
    Path((owner, project)): Path<(String, String)>,
    State(AppState { pool, .. }): State<AppState>,
) -> Response<Body> {
    // check if owner exist

    let project_id = match sqlx::query!(
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
            tracing::debug!("Can't delete project: Owner does not exist");
            let html = render_to_string(move || {
                view! {
                <div id="status" class="flex flex-row gap-4 w-full">
                    <h2 class="text-xl">"State: Running"</h2>
                    <button
                      hx-target="#status"
                      hx-swap="outerHTML"
                      hx-post="/{project}/{owner}/start"
                      class="btn btn-outline btn-sm btn-accent"
                    >
                      Start
                    </button>
                    <h1> Owner does not exist </h1>
                </div>
                }
            })
            .into_owned();

            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(html))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get project_owners: Failed to query database");
            let html = render_to_string(move || {
                view! {
                <div id="status" class="flex flex-row gap-4 w-full">
                    <h2 class="text-xl">"State: Running"</h2>
                    <button
                      hx-target="#status"
                      hx-swap="outerHTML"
                      hx-post="/{project}/{owner}/start"
                      class="btn btn-outline btn-sm btn-accent"
                    >
                      Start
                    </button>
                    <p> "Failed to query database: " {err.to_string()}</p>
                </div>
                }
            })
            .into_owned();

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(html))
                .unwrap();
        }
    };

    let container_name = format!("{owner}-{}", project.trim_end_matches(".git")).replace('.', "-");
    let db_name = format!("{}-db", container_name);

    let docker = match Docker::connect_with_local_defaults() {
        Err(err) => {
            tracing::error!(?err, "Can't delete project: Failed to connect to docker");
            let owner = owner.clone();
            let project = project.clone();

            let html = render_to_string(move || {
                view! {
                <div id="status" class="flex flex-row gap-4 w-full">
                    <h2 class="text-xl">"State: Running"</h2>
                    <button
                      hx-target="#status"
                      hx-swap="outerHTML"
                      hx-post="/{project}/{owner}/start"
                      class="btn btn-outline btn-sm btn-accent"
                    >
                      Start
                    </button>
                    <p> "Failed to connect to docker " {err.to_string() } </p>
                </div>
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

    // Start container
    if let Err(err) = docker
        .start_container(&db_name, None::<StartContainerOptions<&str>>)
        .await
    {
        tracing::error!(?err, "Can't delete project: Failed to start container");
        let html = render_to_string(move || {
            view! {
            <div id="status" class="flex flex-row gap-4 w-full">
                <h2 class="text-xl">"State: Running"</h2>
                <button
                  hx-target="#status"
                  hx-swap="outerHTML"
                  hx-post="/{project}/{owner}/start"
                  class="btn btn-outline btn-sm btn-accent"
                >
                  Start
                </button>
                <p> "Failed to start container " {err.to_string() } </p>
            </div>
            }
        })
        .into_owned();

        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(html))
            .unwrap();
    }

    // start db container
    if let Err(err) = docker
        .start_container(&db_name, None::<StartContainerOptions<&str>>)
        .await
    {
        tracing::error!(?err, "Can't delete project: Failed to start db container");
        let html = render_to_string(move || {
            view! {
            <div id="status" class="flex flex-row gap-4 w-full">
                <h2 class="text-xl">"State: Running"</h2>
                <button
                  hx-target="#status"
                  hx-swap="outerHTML"
                  hx-post="/{project}/{owner}/start"
                  class="btn btn-outline btn-sm btn-accent"
                >
                  Start
                </button>
                <p> "Failed to start db container " {err.to_string() } </p>
            </div>
            }
        })
        .into_owned();

        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(html))
            .unwrap();
    }

    // change state in db
    match sqlx::query!(
        r#"UPDATE projects SET state = 'running' WHERE id = $1"#,
        project_id
    )
    .execute(&pool)
    .await
    {
        Ok(_) => {}
        Err(err) => {
            tracing::error!(?err, "Can't delete project: Failed to update project state");
            let html = render_to_string(move || {
                view! {
                <div id="status" class="flex flex-row gap-4 w-full">
                    <h2 class="text-xl">"State: Running"</h2>
                    <button
                      hx-target="#status"
                      hx-swap="outerHTML"
                      hx-post="/{project}/{owner}/start"
                      class="btn btn-outline btn-sm btn-accent"
                    >
                      Start
                    </button>
                    <p> "Failed to update project state " {err.to_string() } </p>
                </div>
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
        <div id="status" class="flex flex-row gap-4 w-full">
            <h2 class="text-xl">"State: Running"</h2>
            <button
              hx-target="#status"
              hx-swap="outerHTML"
              hx-post="/{project}/{owner}/start"
              class="btn btn-outline btn-sm btn-accent"
            >
              Start
            </button>
            <p> "Project Started" </p>
        </div>
        }
    })
    .into_owned();

    return Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(html))
        .unwrap();
}
