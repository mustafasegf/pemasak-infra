use crate::auth::auth;
use crate::components::Base;
use crate::configuration::Settings;
use crate::{auth::Auth, startup::AppState};
use axum::routing::get;
use axum::{Router, middleware};
use axum::extract::State;
use axum::response::Response;
use axum_extra::routing::RouterExt;
use hyper::{Body, StatusCode};
use leptos::ssr::render_to_string;
use leptos::{view, IntoView, IntoAttribute};

pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route_with_tsr("/dashboard", get(dashboard_ui))
        .route_layer(middleware::from_fn(auth))
}

#[tracing::instrument(skip(auth, pool))]
pub async fn dashboard_ui(
    auth: Auth,
    State(AppState { pool, .. }): State<AppState>,
) -> Response<Body> {
    let user = auth.current_user.unwrap();

    let user_details = match sqlx::query!(
        r#"SELECT name 
        FROM users
        WHERE id = $1"#,
        user.id
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            tracing::error!("Can't get user: User not found with id {}", user.id);
            let html = render_to_string(move || {
                view! {
                    <h1>"User not found"</h1>
                }
            })
            .into_owned();

            return Response::builder()
                .status(403)
                .body(Body::from(html))
                .unwrap();
        }
        Err(err) => {
            tracing::error!(?err, "Can't get user: Failed to query database");
            let html = render_to_string(move || {
                view! {
                    <h1> "Failed to query database "{err.to_string() } </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(500)
                .body(Body::from(html))
                .unwrap();
        }
    };

    let projects = match sqlx::query!(
        r#"SELECT projects.id AS id, projects.name AS project, project_owners.name AS owner
           FROM projects
           JOIN project_owners ON projects.owner_id = project_owners.id
           JOIN users_owners ON project_owners.id = users_owners.owner_id
           JOIN users ON users_owners.user_id = users.id
           WHERE users.id = $1
        "#,
        user.id
    )
    .fetch_all(&pool)
    .await
    {
        Ok(data) => data,
        Err(err) => {
            tracing::error!(?err, "Can't get projects: Failed to query database");
            let html = render_to_string(move || {
                view! {
                    <h1> "Failed to query database "{err.to_string() } </h1>
                }
            })
            .into_owned();
            return Response::builder()
                .status(500)
                .body(Body::from(html))
                .unwrap();
        }
    };

    let html = render_to_string(move || {
        view! {
            <Base>
                <div class="flex items-center justify-between mb-6">
                    <details class="dropdown">
                        <summary class="btn btn-lg px-0 text-left bg-transparent hover:bg-transparent hover:outline-none hover:border-none">                            
                            <div class="flex flex items-center space-x-4">
                                <div class="w-12">
                                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M17.982 18.725A7.488 7.488 0 0012 15.75a7.488 7.488 0 00-5.982 2.975m11.963 0a9 9 0 10-11.963 0m11.963 0A8.966 8.966 0 0112 21a8.966 8.966 0 01-5.982-2.275M15 9.75a3 3 0 11-6 0 3 3 0 016 0z" />
                                    </svg>
                                </div>
                                <div class="flex flex-col justify-center space-y-1">
                                    <p class="font-bold text-xl">{user_details.name}</p>
                                    <p class="text-sm">{user.username}</p>
                                </div>
                            </div>
                        </summary>
                        <ul class="p-2 mt-2 shadow menu dropdown-content z-[1] bg-base-100 rounded-box w-64">
                            <li><a>Item 1</a></li>
                            <li><a>Item 2</a></li>
                        </ul>
                    </details>
        
                    <div class="flex space-x-4">
                        <a href="/new" hx-boost="true">
                            <button class="btn btn-sm btn-outline btn-primary">
                                + New Project
                            </button>
                        </a>
                    </div>
                </div>

                <div class="w-full grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4" hx-boost="true">
                    {projects.into_iter().map(|record|{ view!{
                        <div class="bg-neutral/40 backdrop-blur-sm text-info py-4 px-8 cursor-pointer w-full rounded-lg transition-all outline outline-transparent hover:outline-blue-500 h-36">
                            {
                                let id = record.id.to_string();
                                let name = format!("{}/{}", record.owner, record.project);
                                view! {
                                    <a href={name.clone()} class="text-sm flex flex-col justify-between h-full">
                                        <h2 class="text-lg font-bold text-neutral-content">{name}</h2>
                                        <span class="text-xs text-neutral-accent">{id}</span>
                                    </a>
                                }
                            }
                        </div>
                    }}).collect::<Vec<_>>()}
                </div>
            </Base>
        }
    })
    .into_owned();
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html")
        .body(Body::from(html))
        .unwrap()
}
