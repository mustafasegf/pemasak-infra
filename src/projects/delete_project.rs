use std::collections::HashMap;
use std::fs::File;

use axum::extract::{Path, State};
use axum::response::Response;
use bollard::network::InspectNetworkOptions;
use bollard::Docker;
use hyper::{Body, StatusCode};
use leptos::ssr::render_to_string;
use leptos::{view, IntoView};

use crate::docker;
use crate::startup::AppState;

#[tracing::instrument(skip(pool, base))]
pub async fn post(
    Path((owner, project)): Path<(String, String)>,
    State(AppState { pool, base, .. }): State<AppState>,
) -> Response<Body> {
    fn to_response(status: HashMap<&'static str, &'static str>) -> Response<Body> {
        let success = status.iter().all(|(_, v)| *v == "successfully deleted");
        let el = match success {
            true => {
                view! {
                    <div>
                        <h1> "successfully deleted repo" </h1>
                    </div>
                }
            }
            false => {
                view! {
                   <div>
                   <h1> "some action failed" </h1>
                    {status.into_iter().map(|(k, v)|{ view!{
                        <h1> {k.to_string()} {v.to_string()} </h1>
                    }}).collect::<Vec<_>>() }
                   </div>
                }
            }
        };

        let html = render_to_string(move || {
            view! {
                {el}
                <script>
                r#"
                setTimeout(function() {
                    window.location.href = '/dashboard';
                }, 1000);  // 1000 milliseconds = 1 seconds
            "#
                </script>
            }
        })
        .into_owned();
        Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(html))
            .unwrap()
    }

    let path = match project.ends_with(".git") {
        true => format!("{base}/{owner}/{project}"),
        false => format!("{base}/{owner}/{project}.git"),
    };
    //TODO: better error log

    let mut status: HashMap<&'static str, &'static str> = HashMap::new();

    // check if owner exist
    match sqlx::query!(
        r#"SELECT id FROM project_owners WHERE name = $1 AND deleted_at IS NULL"#,
        owner,
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(data)) => {
            // check if project exist
            match sqlx::query!(
                r#"SELECT id FROM projects WHERE name = $1 AND owner_id = $2"#,
                project,
                data.id,
            )
            .fetch_optional(&pool)
            .await
            {
                Ok(Some(_)) => {
                    match sqlx::query!(
                        "DELETE FROM projects WHERE name = $1 AND owner_id = $2",
                        project,
                        data.id
                    )
                    .execute(&pool)
                    .await
                    {
                        Ok(_) => {
                            status.insert("project", "successfully deleted");
                        }
                        Err(err) => {
                            tracing::error!(?err, "Can't delete project: Failed to delete project");
                            status.insert("project", "failed to delete: database error");
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(?err, "Can't delete project: Failed to query database");
                    status.insert("project", "failed to delete: database error");
                }
                _ => {
                    status.insert("project", "failed to delete: project does not exist");
                }
            };
        }
        Ok(None) => {
            tracing::debug!("Can't delete project: Owner does not exist");
        }
        Err(err) => {
            tracing::error!(?err, "Can't get project_owners: Failed to query database");
        }
    }

    // check if repo exists
    match File::open(&path) {
        Err(err) => {
            tracing::debug!(?err, "Can't delete project: Repo does not exist");
            status.insert("repo", "failed to delete: repo does not exist");
        }
        Ok(_) => match std::fs::remove_dir_all(&path) {
            Ok(_) => {
                status.insert("repo", "successfully deleted");
            }
            Err(err) => {
                tracing::error!(?err, "Can't delete project: Failed to delete repo");
                status.insert("repo", "failed to delete: repo error");
            }
        },
    };

    let container_name = format!("{owner}-{}", project.trim_end_matches(".git")).replace('.', "-");
    let db_name = format!("{}-db", container_name);
    let network_name = format!("{}-network", container_name);
    let volume_name = format!("{}-volume", container_name);

    let docker = match Docker::connect_with_local_defaults() {
        Err(err) => {
            tracing::error!(?err, "Can't delete project: Failed to connect to docker");
            status.insert("container", "failed to delete: docker error");
            return to_response(status);
        }
        Ok(docker) => docker,
    };

    // remove container
    // TODO: check if container have deployed
    match docker::remove_container(&docker, &container_name).await {
        Ok(_) => {
            status.insert("container", "successfully deleted");
        }
        Err(err) => {
            tracing::error!(?err, "Can't delete project: Failed to delete container");
            status.insert("container", "failed to delete: container error");
        }
    };

    // remove image
    match docker.inspect_image(&container_name).await {
        Ok(_) => match docker.remove_image(&container_name, None, None).await {
            Ok(_) => {
                status.insert("image", "successfully deleted");
            }
            Err(err) => {
                tracing::error!(?err, "Can't delete project: Failed to delete image");
                status.insert("image", "failed to delete: image error");
            }
        },
        Err(err) => {
            tracing::debug!(?err, "Can't delete project: Image does not exist");
            status.insert("image", "failed to delete: image does not exist");
        }
    };

    // remove database
    match docker::remove_container(&docker, &db_name).await {
        Ok(_) => {
            status.insert("db", "successfully deleted");
        }
        Err(err) => {
            tracing::error!(?err, "Can't delete project: Failed to delete db");
            status.insert("db", "failed to delete: container error");
        }
    }

    // delete volume
    match docker::remove_volume(&docker, &volume_name).await {
        Ok(_) => {
            status.insert("volume", "successfully deleted");
        }
        Err(err) => {
            tracing::error!(?err, "Can't delete project: Failed to delete volume");
            status.insert("volume", "failed to delete: volume error");
        }
    }

    // remove network
    match docker
        .inspect_network(
            &network_name,
            Some(InspectNetworkOptions::<&str> {
                verbose: true,
                ..Default::default()
            }),
        )
        .await
    {
        Ok(_) => match docker.remove_network(&network_name).await {
            Ok(_) => {
                status.insert("network", "successfully deleted");
            }
            Err(err) => {
                tracing::error!(?err, "Can't delete project: Failed to delete network");
                status.insert("network", "failed to delete: network error");
            }
        },
        Err(err) => {
            tracing::debug!(?err, "Can't delete project: network does not exist");
            status.insert("network", "failed to delete: network does not exist");
        }
    };

    to_response(status)
}
