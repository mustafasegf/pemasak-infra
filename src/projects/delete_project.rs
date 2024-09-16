use std::collections::HashMap;
use std::fs::File;

use axum::extract::{State, Path};
use axum::response::Response;
use bollard::Docker;
use bollard::container::{RemoveContainerOptions, StopContainerOptions};
use bollard::network::InspectNetworkOptions;
use hyper::{Body, StatusCode};
use leptos::ssr::render_to_string;
use leptos::{view, IntoView};
use serde::Serialize;

use crate::auth::Auth;
use crate::startup::AppState;

#[derive(Serialize)]
struct DeleteProjectSuccessResponse {
    message: String
}

#[derive(Serialize)]
struct DeleteProjectErrorResponse {
    message: String,
    details: Vec<String>
}

#[tracing::instrument(skip(pool, base, auth))]
pub async fn post(
    auth: Auth,
    Path((owner, project)): Path<(String, String)>,
    State(AppState { pool, base, .. }): State<AppState>,
) -> Response<Body> {
    fn to_response(status: HashMap<&'static str, &'static str>) -> Response<Body> {
        let success = status.iter().all(|(_, v)| *v == "successfully deleted");
        let json = match success {
            true => serde_json::to_string(
                &DeleteProjectSuccessResponse {
                    message: "Successfully deleted project".to_string(),
                }
            ),
            false => serde_json::to_string(
                &DeleteProjectErrorResponse {
                    message: "Failed to delete project".to_string(),
                    details: status.into_iter().map(|(k, v)|{ format!("{}: {}", k.to_string(), v.to_string()) }).collect::<Vec<_>>()
                }
            )
        }.unwrap();

        Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(json))
            .unwrap()
    }

    let path = match project.ends_with(".git") {
        true => format!("{base}/{owner}/{project}"),
        false => format!("{base}/{owner}/{project}.git"),
    };

    match auth.current_user {
        Some(user) => {
            if user.username != owner {
                let json = serde_json::to_string(&DeleteProjectErrorResponse {
                    message: format!("You are not allowed to delete this project"),
                    details: vec!(),
                }).unwrap();
    
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from(json))
                    .unwrap();
            }
        },
        None => ()
    }

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
    match docker.inspect_container(&container_name, None).await {
        Ok(_) => {
            match docker
                .stop_container(&container_name, None::<StopContainerOptions>)
                .await
            {
                Ok(_) => {
                    match docker
                        .remove_container(&container_name, None::<RemoveContainerOptions>)
                        .await
                    {
                        Ok(_) => {
                            status.insert("container", "successfully deleted");
                        }
                        Err(err) => {
                            tracing::error!(
                                ?err,
                                "Can't delete project: Failed to delete container"
                            );
                            status.insert("container", "failed to delete: container error");
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(?err, "Can't delete project: Failed to stop container");
                    status.insert("container", "failed to delete: container error");
                }
            };
        }
        Err(err) => {
            tracing::debug!(?err, "Can't delete project: Container does not exist");
            status.insert("container", "failed to delete: container does not exist");
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
    match docker.inspect_container(&db_name, None).await {
        Ok(_) => {
            match docker
                .stop_container(&db_name, None::<StopContainerOptions>)
                .await
            {
                Ok(_) => {
                    match docker
                        .remove_container(&db_name, None::<RemoveContainerOptions>)
                        .await
                    {
                        Ok(_) => {
                            status.insert("db", "successfully deleted");
                        }
                        Err(err) => {
                            tracing::error!(?err, "Can't delete project: Failed to delete db");
                            status.insert("db", "failed to delete: container error");
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(?err, "Can't delete project: Failed to stop db");
                    status.insert("db", "failed to delete: container error");
                }
            };
        }
        Err(err) => {
            tracing::debug!(?err, "Can't delete project: db does not exist");
            status.insert("db", "failed to delete: container does not exist");
        }
    };

    // delete volume
    match docker.inspect_volume(&volume_name).await {
        Ok(_) => match docker.remove_volume(&volume_name, None).await {
            Ok(_) => {
                status.insert("volume", "successfully deleted");
            }
            Err(err) => {
                tracing::error!(?err, "Can't delete project: Failed to delete volume");
                status.insert("volume", "failed to delete: volume error");
            }
        },
        Err(err) => {
            tracing::debug!(?err, "Can't delete project: volume does not exist");
            status.insert("volume", "failed to delete: volume does not exist");
        }
    };

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
