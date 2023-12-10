use axum::extract::Path;
use axum::response::Response;
use bollard::container::StartContainerOptions;
use bollard::Docker;
use bollard::volume::CreateVolumeOptions;
use hyper::{Body, StatusCode};
use leptos::ssr::render_to_string;
use leptos::{view, IntoView};

use crate::docker;

#[tracing::instrument]
pub async fn post(Path((owner, project)): Path<(String, String)>) -> Response<Body> {
    let container_name = format!("{owner}-{}", project.trim_end_matches(".git")).replace('.', "-");
    let db_name = format!("{}-db", container_name);
    let volume_name = format!("{}-volume", container_name);

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
                .stop_container(&db_name, None)
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

    // let status = match docker.inspect_volume(&volume_name).await {
    //     Ok(_) => match docker.remove_volume(&volume_name, None).await {
    //         Ok(_) => "successfully deleted",
    //         Err(err) => {
    //             tracing::error!(?err, "Can't delete volume: Failed to delete volume");
    //             "failed to delete: volume error"
    //         }
    //     },
    //     Err(err) => {
    //         tracing::debug!(?err, "Can't delete volume: volume does not exist");
    //         "failed to delete: volume does not exist"
    //     }
    // };

    let mut status = match docker::remove_volume(&docker, &volume_name).await {
        Ok(_) => "successfully deleted",
        Err(err) => {
            tracing::error!(?err, "Can't delete volume: Failed to delete volume");
            "failed to delete: volume error"
        }
    };

    if let Err(err) = docker.create_volume(
        CreateVolumeOptions {
            name: &volume_name,
            driver: &"local".to_string(),
            driver_opts: Default::default(),
            labels: Default::default(),
        },
    ).await {
        tracing::error!(?err, "Can't delete volume: Failed to create volume");
        status = "failed to delete: volume error";
    }


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

    let html = render_to_string(move || {
        view! {
            <div>
                <h1> {status} </h1>
            </div>
        }
    })
    .into_owned();
    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(html))
        .unwrap()
}
