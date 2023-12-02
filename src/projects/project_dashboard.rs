use std::fmt;

use axum::extract::{State, Path};
use axum::response::Response;
use hyper::{Body, StatusCode};
use leptos::ssr::render_to_string;
use leptos::{view, IntoView};
use serde::{Serialize, Deserialize};

use crate::components::Base;
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
                    <Base>
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
        }, 
    };

    let html = render_to_string(move || {
        view! {
            <Base>
              <div class="flex items-center justify-between mb-6">
                <div class="flex flex items-center">
                    <div class="flex flex-col justify-center space-y-1">
                        <p class="font-bold text-xl">{&owner}"/"{&project}</p>
                    </div>
                </div>
                <div class="flex space-x-4">
                    <a href="" hx-boost="true">
                        <button class="btn btn-sm btn-outline btn-secondary gap-1">
                          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M10.5 6h9.75M10.5 6a1.5 1.5 0 11-3 0m3 0a1.5 1.5 0 10-3 0M3.75 6H7.5m3 12h9.75m-9.75 0a1.5 1.5 0 01-3 0m3 0a1.5 1.5 0 00-3 0m-3.75 0H7.5m9-6h3.75m-3.75 0a1.5 1.5 0 01-3 0m3 0a1.5 1.5 0 00-3 0m-9.75 0h9.75" />
                          </svg>
                          Settings
                        </button>
                    </a>
                    <a href={format!("http://{}-{}.{}", &owner, &project, &domain)} target="_blank" rel="noreferrer">
                      <button class="btn btn-sm btn-outline btn-primary gap-1">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5"><path stroke-linecap="round" stroke-linejoin="round" d="M15.75 9V5.25A2.25 2.25 0 0013.5 3h-6a2.25 2.25 0 00-2.25 2.25v13.5A2.25 2.25 0 007.5 21h6a2.25 2.25 0 002.25-2.25V15m3 0l3-3m0 0l-3-3m3 3H9"></path></svg>
                        Open
                      </button>
                    </a>
                </div>
              </div>

            // TODO: Move this to a separate page accessible by the "Settings" button
            //   <h2 class="text-xl">
            //     Project Controls
            //   </h2>
            //   <div class="flex w-full space-x-4 items-center mb-8">
            //     <button
            //       hx-post={delete_path}
            //       hx-trigger="click"
            //       class="btn btn-error mt-4 w-full max-w-xs"
            //     >Delete Project</button>

            //     <button
            //       hx-post={volume_path}
            //       hx-trigger="click"
            //       class="btn btn-error mt-4 w-full max-w-xs"
            //     >Delete Database</button>
            //   </div>

              <div id="result"></div>

              <h2 class="text-xl">
                Builds
              </h2>
              <div class="flex flex-col gap-4 w-full mt-4">
                {
                    match builds.len() > 0 {
                        true => {
                            builds.into_iter().enumerate().map(|(index, record)| { view!{
                                <div class="bg-neutral/40 backdrop-blur-sm text-info py-4 px-8 cursor-pointer w-full rounded-lg transition-all outline outline-transparent hover:outline-blue-500">
                                    {
                                        let id = record.id.to_string();
                                        let status = record.status.to_string();
                                        let created_at = record.created_at;
                                        view!{
                                            <a class="text-sm">
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
                                            </a>
                                        }
                                    }
                                </div>
                            }}).collect::<Vec<_>>()
                        },
                        false => {
                            vec!(
                                view! {
                                    <div>
                                        <p class="mb-4">You have not pushed a build to your project, to push an existing project, execute the following command in your project</p>
                                        <div class="p-4 mb-4 bg-gray-800 mockup-code" id="code">
                                            <pre>
                                                <code>
                                                    git remote add pws {format!(" http://{}/{}/{}", domain, owner, project)}
                                                </code>
                                            </pre>
                                            <pre>
                                                <code>
                                                    {"git push -u pws master"}
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
