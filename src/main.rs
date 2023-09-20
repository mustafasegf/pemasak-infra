use std::collections::HashMap;

use anyhow::Result;
use bollard::{
    container::{
        Config, CreateContainerOptions, ListContainersOptions, StartContainerOptions,
        StopContainerOptions,
    },
    image::ListImagesOptions,
    Docker,
};
use nixpacks::{
    create_docker_image,
    nixpacks::{builder::docker::DockerBuilderOptions, plan::generator::GeneratePlanOptions},
};

#[tokio::main]
async fn main() -> Result<()> {
    let container_name = "go-example".to_string();
    let image_name = "go-example:latest".to_string();
    let container_src = "./src/go-example".to_string();

    let plan_options = GeneratePlanOptions::default();
    let build_options = DockerBuilderOptions {
        name: Some(container_name.clone()),
        quiet: false,
        ..Default::default()
    };
    let envs = vec![];
    create_docker_image(&container_src, envs, &plan_options, &build_options).await?;

    let docker = Docker::connect_with_local_defaults()?;

    let images = &docker
        .list_images(Some(ListImagesOptions::<String> {
            all: false,
            filters: HashMap::from([("reference".to_string(), vec![image_name.clone()])]),
            ..Default::default()
        }))
        .await
        .unwrap();

    for image in images {
        println!("images -> {:#?}", image);
    }

    let _image = images.first().ok_or(anyhow::anyhow!("No image found"))?;

    // remove container if it exists
    let containers = docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            filters: HashMap::from([("name".to_string(), vec![container_name.clone()])]),
            ..Default::default()
        }))
        .await?
        .into_iter()
        .collect::<Vec<_>>();

    for container in &containers {
        println!("container -> {:#?}", container.names);
    }

    if !containers.is_empty() {
        docker
            .stop_container(&container_name, None::<StopContainerOptions>)
            .await?;

        docker
            .remove_container(
                containers.first().unwrap().id.as_ref().unwrap(),
                None::<bollard::container::RemoveContainerOptions>,
            )
            .await?;
    }

    let config = Config {
        image: Some(image_name.clone()),
        ..Default::default()
    };

    let res = &docker
        .create_container(
            Some(CreateContainerOptions {
                name: container_name.clone(),
                platform: None,
            }),
            config,
        )
        .await?;

    println!("create response-> {:#?}", res);

    docker
        .start_container(&container_name, None::<StartContainerOptions<String>>)
        .await?;

    Ok(())
}
