use std::{collections::HashMap, sync::RwLock};

use anyhow::Result;
use bollard::{
    container::{
        Config, CreateContainerOptions, ListContainersOptions, StartContainerOptions,
        StopContainerOptions,
    },
    image::ListImagesOptions,
    network::{ConnectNetworkOptions, InspectNetworkOptions, ListNetworksOptions},
    Docker,
};
use nixpacks::{
    create_docker_image,
    nixpacks::{builder::docker::DockerBuilderOptions, plan::generator::GeneratePlanOptions},
};

lazy_static::lazy_static! {
    pub static ref REGISTERED_ROUTES: RwLock<HashMap<String, String>> = {
        RwLock::new([("go-example".to_string(), "172.31.0.2:8080".to_string())].into() )
    };
}

pub async fn build_docker(container_name: &str, container_src: &str) -> Result<()> {
    let image_name = format!("{}:latest", container_name);
    let network_name = format!("{}-network", container_name);

    let plan_options = GeneratePlanOptions::default();
    let build_options = DockerBuilderOptions {
        name: Some(container_name.to_string()),
        quiet: false,
        ..Default::default()
    };
    let envs = vec![];
    create_docker_image(&container_src, envs, &plan_options, &build_options).await?;

    let docker = Docker::connect_with_local_defaults()?;

    let images = &docker
        .list_images(Some(ListImagesOptions::<String> {
            all: false,
            filters: HashMap::from([("reference".to_string(), vec![image_name.to_string()])]),
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
            filters: HashMap::from([("name".to_string(), vec![container_name.to_string()])]),
            ..Default::default()
        }))
        .await?
        .into_iter()
        .collect::<Vec<_>>();

    for container in &containers {
        println!("container -> {:?}", container.names);
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

    // check if network exists
    let network = docker
        .list_networks(Some(ListNetworksOptions {
            filters: HashMap::from([("name".to_string(), vec![network_name.to_string()])]),
        }))
        .await?
        .first()
        .map(|n| n.to_owned());

    let network = match network {
        Some(n) => {
            println!("Existing network id -> {:?}", n.id);
            n
        }
        None => {
            let options = bollard::network::CreateNetworkOptions {
                name: network_name.clone(),
                ..Default::default()
            };
            let res = docker.create_network(options).await?;
            println!("create network response-> {:#?}", res);

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
        .await?;

    println!("connect network response-> {:#?}", res);

    docker
        .start_container(&container_name, None::<StartContainerOptions<String>>)
        .await?;

    //inspect network
    let network_inspect = docker
        .inspect_network(
            &network.id.unwrap(),
            Some(InspectNetworkOptions::<&str> {
                verbose: true,
                ..Default::default()
            }),
        )
        .await?;

    println!(
        "ipv4 address -> {:#?}",
        network_inspect
            .containers
            .unwrap()
            .get(&res.id)
            .unwrap()
            .ipv4_address
    );
    Ok(())
}
