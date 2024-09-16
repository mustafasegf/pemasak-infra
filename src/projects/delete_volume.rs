use axum::extract::Path;
use axum::response::Response;
use bollard::Docker;
use bollard::container::{StopContainerOptions, StartContainerOptions};
use hyper::{Body, StatusCode};
use serde::Serialize;
use crate::auth::Auth;

#[derive(Serialize)]
struct DeleteVolumeSuccessResponse {
    message: String
}

#[derive(Serialize)]
struct DeleteVolumeErrorResponse {
    message: String,
    details: Vec<String>
}

#[tracing::instrument(skip(auth))]
pub async fn post(auth: Auth, Path((owner, project)): Path<(String, String)>) -> Response<Body> {
    let container_name = format!("{owner}-{}", project.trim_end_matches(".git")).replace('.', "-");
    let db_name = format!("{}-db", container_name);
    let volume_name = format!("{}-volume", container_name);

    match auth.current_user {
        Some(user) => {
            if user.username != owner {
                let json = serde_json::to_string(&DeleteVolumeErrorResponse {
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

    let docker = match Docker::connect_with_local_defaults() {
        Ok(docker) => docker,
        Err(err) => {
            tracing::error!(?err, "Can't delete volume: Failed to connect to docker");
            // TODO: better message
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(""))
                .unwrap();
        }
    };

    let turned_on = match docker.inspect_container(&db_name, None).await {
        Ok(_) => {
            match docker
                .stop_container(&db_name, None::<StopContainerOptions>)
                .await
            {
                Ok(_) => true,
                Err(err) => {
                    tracing::error!(?err, "Can't delete volume: Failed to stop db");
                    false
                }
            }
        }
        Err(err) => {
            tracing::debug!(?err, "Can't delete volume: db does not exist");
            false
        }
    };

    let status = match docker.inspect_volume(&volume_name).await {
        Ok(_) => match docker.remove_volume(&volume_name, None).await {
            Ok(_) => "successfully deleted",
            Err(err) => {
                tracing::error!(?err, "Can't delete volume: Failed to delete volume");
                "failed to delete: volume error"
            }
        },
        Err(err) => {
            tracing::debug!(?err, "Can't delete volume: volume does not exist");
            "failed to delete: volume does not exist"
        }
    };

    if turned_on {
        match docker
            .start_container(&db_name, None::<StartContainerOptions<&str>>)
            .await
        {
            Ok(_) => {}
            Err(err) => {
                tracing::error!(?err, "Can't delete volume: Failed to start db");
            }
        }
    }

    let json = serde_json::to_string(
        &DeleteVolumeSuccessResponse {
            message: status.to_string()
        }
    ).unwrap();

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(json))
        .unwrap()
}
