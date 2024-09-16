use crate::auth::auth;
use crate::configuration::Settings;
use crate::{auth::Auth, startup::AppState};
use axum::routing::get;
use axum::{Router, middleware};
use axum::extract::State;
use axum::response::Response;
use axum_extra::routing::RouterExt;
use hyper::Body;
use leptos::ssr::render_to_string;
use leptos::{view, IntoView};
use serde::Serialize;
use uuid::Uuid;

pub async fn router(_state: AppState, _config: &Settings) -> Router<AppState, Body> {
    Router::new()
        .route_with_tsr("/api/dashboard/project", get(get_dashboard_projects))
        .route_layer(middleware::from_fn(auth))
}

#[derive(Serialize, Debug)]
struct Project {
    id: Uuid,
    name: String,
    owner_name: String,
}

#[derive(Serialize, Debug)]
struct DashboardProjectResponse {
    data: Vec<Project>
}
pub async fn get_dashboard_projects(auth: Auth, State(AppState { pool, .. }): State<AppState>) -> Response<Body> {
    let user = auth.current_user.unwrap();

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

    let projects = projects.into_iter().map(|record|{ 
        Project {
            id: record.id,
            name: record.project,
            owner_name: record.owner,
        }
    }).collect::<Vec<_>>();

    Response::builder()
        .status(200)
        .body(
            Body::from(serde_json::to_string(
                &DashboardProjectResponse {
                    data: projects
                }
            ).unwrap())
        )
        .unwrap()
} 
