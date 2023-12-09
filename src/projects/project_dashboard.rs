use std::fmt;

use axum::extract::{Path, State};
use axum::response::Response;
use hyper::{Body, StatusCode};
use leptos::ssr::render_to_string;
use leptos::{view, IntoAttribute, IntoView};
use serde::{Deserialize, Serialize};

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
    Path((owner, project)): Path<(String, String)>,
) -> Response<Body> {
    let _user = auth.current_user.unwrap();

    // check if project exist
    let project_record = match sqlx::query!(
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

    let builds = match sqlx::query!(
        r#"SELECT id, project_id, status AS "status: BuildState", created_at, finished_at 
        FROM builds WHERE project_id = $1
        ORDER BY created_at DESC"#,
        project_record.id
    )
    .fetch_all(&pool)
    .await
    {
        Ok(records) => records,
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
                Builds
              </h2>
              <div class="flex flex-col gap-4 w-full mt-4">
                {
                    match !builds.is_empty() {
                        true => {
                            builds.into_iter().enumerate().map(|(index, record)| { view!{
                                <>
                                    <a hx-boost="true" href={format!("/{}/{}/builds/{}", owner, project, record.id)} class="bg-neutral/40 backdrop-blur-sm text-info py-4 px-8 cursor-pointer w-full rounded-lg transition-all outline outline-transparent hover:outline-blue-500">
                                        {
                                            let id = record.id.to_string();
                                            let status = record.status.to_string();
                                            let created_at = record.created_at;
                                            view!{
                                                <div class="text-sm">
                                                    <h2 class="font-bold text-white">
                                                        <span>{id}</span>
                                                        {
                                                            let latest_build = match index == 0 {
                                                                true => " (LATEST BUILD)",
                                                                false => ""
                                                            };
                
                                                            view!{
                                                                <span class="text-info">{latest_build}</span>
                                                            }
                                                        }
                                                    </h2>
                                                    <p class="text-sm text-neutral-content">{"Status: "}{status}</p>
                                                    <p class="text-sm text-neutral-content">{"Started at: "}{created_at.to_rfc2822()}</p>
                                                </div>
                                            }
                                        }
                                    </a>
                                </>
                            }}).collect::<Vec<_>>()
                        },
                        false => {
                            let protocol = match secure {
                                true => "https",
                                false => "http"
                            };
                            
                            vec!(
                                view! {
                                    <>
                                        <div>
                                            <p class="mb-4">You have not pushed a build to your project, to push an existing project, execute the following command in your project</p>
                                            <div class="p-4 mb-4 bg-neutral/40 backdrop-blur-sm mockup-code" id="code">
                                                <pre>
                                                    <code>
                                                        "git remote add pws" {format!(" {protocol}://{domain}/{owner}/{project}")}
                                                    </code>
                                                </pre>
                                                <pre>
                                                    <code>
                                                        "git branch -M master" 
                                                    </code>
                                                </pre>
                                                <pre>
                                                    <code>
                                                        {"git push pws master"}
                                                    </code>
                                                </pre>
                                            </div>
                                            <button
                                                class="btn btn-outline btn-secondary mb-4"
                                                onclick="
                                                let lb = '\\n'
                                                if(navigator.userAgent.indexOf('Windows') != -1) {{
                                                lb = '\\r\\n'
                                                }}
                                
                                                let text = document.getElementById('code').innerText.replaceAll('\\n', lb)
                                                if ('clipboard' in window.navigator) {{
                                                    navigator.clipboard.writeText(text)
                                                }}
                                            "
                                            >
                                            Copy to clipboard
                                            </button>
                                        </div>
                                    </>
                                }
                            )
                        }
                    }
                }
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
