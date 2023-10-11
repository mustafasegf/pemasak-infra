use std::collections::HashMap;

use anyhow::Result;
use bollard::{
    container::{
        Config, CreateContainerOptions, ListContainersOptions, StartContainerOptions,
        StopContainerOptions,
    },
    image::{ListImagesOptions, TagImageOptions},
    network::{ConnectNetworkOptions, InspectNetworkOptions, ListNetworksOptions},
    Docker,
};
use nixpacks::{
    create_docker_image,
    nixpacks::{builder::docker::DockerBuilderOptions, plan::generator::GeneratePlanOptions},
};

#[tracing::instrument]
pub async fn build_docker(container_name: &str, container_src: &str) -> Result<String> {
    let image_name = format!("{}:latest", container_name);
    let old_image_name = format!("{}:old", container_name);
    let network_name = format!("{}-network", container_name);

    let docker = Docker::connect_with_local_defaults().map_err(|err| {
        tracing::error!("Failed to connect to docker: {}", err);
        err
    })?;

    let images = &docker
        .list_images(Some(ListImagesOptions::<String> {
            all: false,
            filters: HashMap::from([("reference".to_string(), vec![image_name.to_string()])]),
            ..Default::default()
        }))
        .await
        .map_err(|err| {
            tracing::error!("Failed to list images: {}", err);
            err
        })?;

    if let Some(_image) = images.first() {
        let tag_options = TagImageOptions {
            tag: "old",
            repo: container_name,
        };

        docker
            .tag_image(container_name, Some(tag_options))
            .await
            .map_err(|err| {
                tracing::error!("Failed to tag image: {}", err);
                err
            })?;

        docker
            .remove_image(&image_name, None, None)
            .await
            .map_err(|err| {
                tracing::error!("Failed to remove image: {}", err);
                err
            })?;
    };

    let plan_options = GeneratePlanOptions::default();
    let build_options = DockerBuilderOptions {
        name: Some(container_name.to_string()),
        quiet: false,
        ..Default::default()
    };
    let envs = vec![];
    if let Err(err) = create_docker_image(container_src, envs, &plan_options, &build_options).await
    {
        tracing::error!("Failed to build docker image: {}", err);
        return Err(err);
    };

    let images = &docker
        .list_images(Some(ListImagesOptions::<String> {
            all: false,
            filters: HashMap::from([("reference".to_string(), vec![image_name.to_string()])]),
            ..Default::default()
        }))
        .await
        .map_err(|err| {
            tracing::error!("Failed to list images: {}", err);
            err
        })?;

    let _image = images.first().ok_or(anyhow::anyhow!("No image found"))?;

    // remove container if it exists
    let containers = docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            filters: HashMap::from([("name".to_string(), vec![container_name.to_string()])]),
            ..Default::default()
        }))
        .await
        .map_err(|err| {
            tracing::error!("Failed to list containers: {}", err);
            err
        })?
        .into_iter()
        .collect::<Vec<_>>();

    if !containers.is_empty() {
        docker
            .stop_container(container_name, None::<StopContainerOptions>)
            .await
            .map_err(|err| {
                tracing::error!("Failed to stop container: {}", err);
                err
            })?;

        docker
            .remove_container(
                containers.first().unwrap().id.as_ref().unwrap(),
                None::<bollard::container::RemoveContainerOptions>,
            )
            .await
            .map_err(|err| {
                tracing::error!("Failed to remove container: {}", err);
                err
            })?;

        docker
            .remove_image(&old_image_name, None, None)
            .await
            .map_err(|err| {
                tracing::error!("Failed to remove image: {}", err);
                err
            })?;
    }

    let config = Config {
        image: Some(image_name.clone()),
        // TDDO: rethink if we need to make this configurable
        env: Some(vec!["PORT=80".to_string()]),
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
        .await
        .map_err(|err| {
            tracing::error!("Failed to create container: {}", err);
            err
        })?;

    tracing::info!("create response-> {:#?}", res);

    // check if network exists
    let network = docker
        .list_networks(Some(ListNetworksOptions {
            filters: HashMap::from([("name".to_string(), vec![network_name.to_string()])]),
        }))
        .await
        .map_err(|err| {
            tracing::error!("Failed to list networks: {}", err);
            err
        })?
        .first()
        .map(|n| n.to_owned());

    let network = match network {
        Some(n) => {
            tracing::info!("Existing network id -> {:?}", n.id);
            n
        }
        None => {
            let options = bollard::network::CreateNetworkOptions {
                name: network_name.clone(),
                ..Default::default()
            };
            let res = docker.create_network(options).await.map_err(|err| {
                tracing::error!("Failed to create network: {}", err);
                err
            })?;
            tracing::info!("create network response-> {:#?}", res);

            docker
                .list_networks(Some(ListNetworksOptions {
                    filters: HashMap::from([("name".to_string(), vec![network_name.to_string()])]),
                }))
                .await?
                .first()
                .map(|n| n.to_owned())
                .ok_or(anyhow::anyhow!("No network found after make one???"))?
        }
    };

    // connect container to network
    docker
        .connect_network(
            &network_name,
            ConnectNetworkOptions {
                container: container_name.clone(),
                ..Default::default()
            },
        )
        .await
        .map_err(|err| {
            tracing::error!("Failed to connect network: {}", err);
            err
        })?;

    tracing::info!("connect network response-> {:#?}", res);

    // TODO: put in port env
    docker
        .start_container(container_name, None::<StartContainerOptions<String>>)
        .await
        .map_err(|err| {
            tracing::error!("Failed to start container: {}", err);
            err
        })?;

    //inspect network
    let network_inspect = docker
        .inspect_network(
            &network.id.unwrap(),
            Some(InspectNetworkOptions::<&str> {
                verbose: true,
                ..Default::default()
            }),
        )
        .await
        .map_err(|err| {
            tracing::error!("Failed to inspect network: {}", err);
            err
        })?;

    let ip = network_inspect
        .containers
        .unwrap_or_default()
        .get(&res.id)
        .unwrap()
        .ipv4_address
        .clone()
        .unwrap_or_default();

    tracing::info!(ip);
    Ok(ip)
}
