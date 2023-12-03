use std::collections::HashMap;
use std::process::Output;

use anyhow::Result;
use bollard::{
    container::{Config, CreateContainerOptions, ListContainersOptions, StartContainerOptions},
    image::{ListImagesOptions, TagImageOptions},
    network::{ConnectNetworkOptions, InspectNetworkOptions, ListNetworksOptions},
    service::{HostConfig, NetworkContainer, RestartPolicy, RestartPolicyNameEnum},
    volume::{CreateVolumeOptions, ListVolumesOptions},
    Docker,
};
use nixpacks::{
    create_docker_image,
    nixpacks::{builder::docker::DockerBuilderOptions, plan::generator::GeneratePlanOptions},
};
use procfile;
use rand::{Rng, SeedableRng};
use sqlx::PgPool;

const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub struct DockerContainer {
    pub ip: String,
    pub port: i32,
    pub build_log: String,
    pub db_url: String,
}

#[tracing::instrument(skip(pool))]
pub async fn build_docker(
    project_name: &str,
    container_name: &str,
    container_src: &str,
    pool: PgPool,
) -> Result<DockerContainer> {
    let image_name = format!("{}:latest", container_name);
    let old_image_name = format!("{}:old", container_name);
    let network_name = format!("{}-network", container_name);
    let db_name = format!("{}-db", container_name);
    let volume_name = format!("{}-volume", container_name);

    let docker = Docker::connect_with_local_defaults().map_err(|err| {
        tracing::error!("Failed to connect to docker: {}", err);
        err
    })?;

    // check if image exists
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

    // remove image if it exists
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

    // build image
    let plan_options = GeneratePlanOptions::default();
    let build_options = DockerBuilderOptions {
        name: Some(container_name.to_string()),
        quiet: false,
        verbose: true,
        ..Default::default()
    };
    let envs = vec![];

    // check if Dockerfile exists

    tracing::info!("BUILDING START");

    let (build_log, nixpacks) = match std::path::Path::new(container_src)
        .join("Dockerfile")
        .exists()
    {
        true => {
            tracing::debug!(container_name, "Build using dockerfile");
            // build from Dockerfile
            match std::process::Command::new("docker")
                .args(&[
                    "build",
                    "-t",
                    &image_name,
                    "-f",
                    &std::path::Path::new(container_src)
                        .join("Dockerfile")
                        .to_str()
                        .unwrap(),
                    container_src,
                ])
                .output()
            {
                Ok(output) => (String::from_utf8(output.stderr).unwrap(), false),
                Err(err) => {
                    tracing::error!("Failed to build image: {}", err);
                    return Err(anyhow::anyhow!("Failed to build image: {}", err));
                }
            }
        }
        false => {
            tracing::debug!(container_name, "Build using nixpacks");
            let container_src = container_src.to_string();
            let Output {
                status,
                stderr,
                stdout: _,
            } = tokio::task::spawn_blocking(move || {
                create_docker_image(&container_src, envs, &plan_options, &build_options)
            }).await??;

            let build_log = String::from_utf8(stderr).unwrap();

            if !status.success() {
                return Err(anyhow::anyhow!(build_log));
            }
            (build_log, true)
        }
    };

    // check if image exists
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

    // check if container exists
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

    // remove container if it exists
    if !containers.is_empty() {
        docker
            .stop_container(container_name, None)
            .await
            .map_err(|err| {
                tracing::error!("Failed to stop container: {}", err);
                err
            })?;

        docker
            .remove_container(containers.first().unwrap().id.as_ref().unwrap(), None)
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

    // check if database container exists
    let db_containers = docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            filters: HashMap::from([("name".to_string(), vec![db_name.to_string()])]),
            ..Default::default()
        }))
        .await
        .map_err(|err| {
            tracing::error!("Failed to list containers: {}", err);
            err
        })?
        .into_iter()
        .collect::<Vec<_>>();

    let volumes = docker
        .list_volumes(Some(ListVolumesOptions::<String> {
            filters: HashMap::from([("name".to_string(), vec![volume_name.clone()])]),
        }))
        .await
        .map_err(|err| {
            tracing::error!("Failed to list containers: {}", err);
            err
        })?
        .volumes
        .unwrap_or_default();

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

    // create network if it doesn't exist
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

    // create volume if it doesn't exist
    if volumes.is_empty() {
        let res = docker
            .create_volume(CreateVolumeOptions {
                name: volume_name.clone(),
                ..Default::default()
            })
            .await
            .map_err(|err| {
                tracing::error!("Failed to create volume: {}", err);
                err
            })?;
        tracing::info!("create volume response-> {:#?}", res);
    }

    // create database container if it doesn't exist
    let db_url = match db_containers.is_empty() {
        true => {
            let mut rng = rand::rngs::StdRng::from_entropy();
            let username = (0..10)
                .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
                .collect::<String>();

            let password = (0..20)
                .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
                .collect::<String>();

            // create database container
            let config = Config {
                image: Some("postgres:16.0-alpine3.18".to_string()),
                volumes: Some(HashMap::from([(
                    format!("{volume_name}:/var/lib/postgresql/data"),
                    HashMap::new(),
                )])),
                env: Some(vec![
                    format!("POSTGRES_USER={}", username),
                    format!("POSTGRES_PASSWORD={}", password),
                    format!("POSTGRES_DB={}", "postgres"),
                ]),
                host_config: Some(HostConfig {
                    restart_policy: Some(RestartPolicy {
                        name: Some(RestartPolicyNameEnum::ON_FAILURE),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            };

            let _res = &docker
                .create_container(
                    Some(CreateContainerOptions {
                        name: db_name.clone(),
                        platform: None,
                    }),
                    config,
                )
                .await
                .map_err(|err| {
                    tracing::error!("Failed to create container: {}", err);
                    err
                })?;

            docker
                .start_container(&db_name, None::<StartContainerOptions<&str>>)
                .await
                .map_err(|err| {
                    tracing::error!("Failed to start container: {}", err);
                    err
                })?;

            // wait until postgres is ready
            // TODO: change this into a psql command check
            std::thread::sleep(std::time::Duration::from_secs(10));

            // connect db container to network
            docker
                .connect_network(
                    &network_name,
                    ConnectNetworkOptions {
                        container: db_name.clone(),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|err| {
                    tracing::error!("Failed to connect network: {}", err);
                    err
                })?;

            format!(
                "postgresql://{}:{}@{}:{}/{}",
                username, password, db_name, 5432, "postgres"
            )
        }
        false => {
            match sqlx::query!(
                r#"SELECT db_url FROM domains
                   JOIN projects ON projects.id = domains.project_id
                   WHERE projects.name = $1
                "#,
                project_name
            )
            .fetch_optional(&pool)
            .await
            {
                Ok(Some(row)) => row.db_url.unwrap(),
                Ok(None) => {
                    // delete database create again
                    tracing::debug!("No database url found for project {}", project_name);

                    let _ = docker.stop_container(&db_name, None).await.map_err(|err| {
                        tracing::error!("Failed to remove container: {}", err);
                        err
                    });

                    let _ = docker
                        .remove_container(&db_name, None)
                        .await
                        .map_err(|err| {
                            tracing::error!("Failed to remove container: {}", err);
                            err
                        });

                    let _ = docker
                        .remove_volume(&volume_name, None)
                        .await
                        .map_err(|err| {
                            tracing::error!("Failed to remove volume: {}", err);
                            err
                        })?;

                    let mut rng = rand::rngs::StdRng::from_entropy();
                    let username = (0..10)
                        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
                        .collect::<String>();

                    let password = (0..20)
                        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
                        .collect::<String>();

                    // create database container
                    let config = Config {
                        image: Some("postgres:16.0-alpine3.18".to_string()),
                        volumes: Some(HashMap::from([(
                            format!("{volume_name}:/var/lib/postgresql/data"),
                            HashMap::new(),
                        )])),
                        env: Some(vec![
                            format!("POSTGRES_USER={}", username),
                            format!("POSTGRES_PASSWORD={}", password),
                            format!("POSTGRES_DB={}", "postgres"),
                        ]),
                        host_config: Some(HostConfig {
                            restart_policy: Some(RestartPolicy {
                                name: Some(RestartPolicyNameEnum::ON_FAILURE),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    };

                    let _res = &docker
                        .create_container(
                            Some(CreateContainerOptions {
                                name: db_name.clone(),
                                platform: None,
                            }),
                            config,
                        )
                        .await
                        .map_err(|err| {
                            tracing::error!("Failed to create container: {}", err);
                            err
                        })?;

                    docker
                        .start_container(&db_name, None::<StartContainerOptions<&str>>)
                        .await
                        .map_err(|err| {
                            tracing::error!("Failed to start container: {}", err);
                            err
                        })?;

                    // wait until postgres is ready
                    // TODO: change this into a psql command check
                    std::thread::sleep(std::time::Duration::from_secs(10));

                    // connect db container to network
                    docker
                        .connect_network(
                            &network_name,
                            ConnectNetworkOptions {
                                container: db_name.clone(),
                                ..Default::default()
                            },
                        )
                        .await
                        .map_err(|err| {
                            tracing::error!("Failed to connect network: {}", err);
                            err
                        })?;

                    format!(
                        "postgresql://{}:{}@{}:{}/{}",
                        username, password, db_name, 5432, "postgres"
                    )
                }
                Err(err) => {
                    tracing::error!("Failed to query database: {}", err);
                    return anyhow::Result::Err(err.into());
                }
            }
        }
    };

    // TODO: figure out if we need make this configurable
    let port = 80;

    let mut config = Config {
        image: Some(image_name.clone()),
        // TDDO: rethink if we need to make this configurable
        env: Some(vec![
            "PRODUCTION=true".to_string(),
            format!("PORT={}", port),
            format!("DATABASE_URL={}", db_url),
        ]),
        host_config: Some(HostConfig {
            restart_policy: Some(RestartPolicy {
                name: Some(RestartPolicyNameEnum::ON_FAILURE),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    // if not nixpacks, we need to read from procfile and use release and web command
    if !nixpacks {
        // read procfile
        let (release, web) =
            std::fs::read_to_string(std::path::Path::new(container_src).join("Procfile"))
                .map(|content| {
                    procfile::parse(&content)
                        .map_err(|err| {
                            tracing::error!("Failed to parse Procfile: {}", err);
                            err
                        })
                        .map(|map| {
                            let web = map.get("web").map(|web| web.to_string());
                            let release = map.get("release").map(|release| release.to_string());
                            (release, web)
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default();

        tracing::debug!(release = ?release, web = ?web, "Procfile");

        if let Some(release) = release {
            let config = Config {
                image: Some(image_name.clone()),
                env: Some(vec![
                    "PRODUCTION=true".to_string(),
                    format!("PORT={}", port),
                    format!("DATABASE_URL={}", db_url),
                ]),
                host_config: Some(HostConfig {
                    restart_policy: Some(RestartPolicy {
                        name: Some(RestartPolicyNameEnum::NO),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                // cmd: Some(vec![release]),
                cmd: Some(release.split(' ').map(|s| s.to_string()).collect()),
                ..Default::default()
            };
            if let Err(err) = docker
                .create_container(
                    Some(CreateContainerOptions {
                        name: container_name.clone(),
                        platform: None,
                    }),
                    config,
                )
                .await
            {
                tracing::error!("Failed to create container: {}", err);
                if db_containers.is_empty() {
                    let _ = docker.stop_container(&db_name, None).await.map_err(|err| {
                        tracing::error!("Failed to remove container: {}", err);
                        err
                    });

                    let _ = docker
                        .remove_container(&db_name, None)
                        .await
                        .map_err(|err| {
                            tracing::error!("Failed to remove container: {}", err);
                            err
                        });

                    let _ = docker
                        .remove_volume(&volume_name, None)
                        .await
                        .map_err(|err| {
                            tracing::error!("Failed to remove volume: {}", err);
                            err
                        });
                }

                return Err(err.into());
            }

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

            if let Err(err) = docker
                .start_container(container_name, None::<StartContainerOptions<&str>>)
                .await
            {
                tracing::error!("Failed to start container: {}", err);

                if db_containers.is_empty() {
                    let _ = docker.stop_container(&db_name, None).await.map_err(|err| {
                        tracing::error!("Failed to remove container: {}", err);
                        err
                    });

                    let _ = docker
                        .remove_container(&db_name, None)
                        .await
                        .map_err(|err| {
                            tracing::error!("Failed to remove container: {}", err);
                            err
                        });

                    let _ = docker
                        .remove_volume(&volume_name, None)
                        .await
                        .map_err(|err| {
                            tracing::error!("Failed to remove volume: {}", err);
                            err
                        });
                }
            }

            // wait until container is stopped
            let mut i = 0;
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));

                if let Err(err) = docker.remove_container(container_name, None).await {
                    tracing::debug!("Failed to remove container. Will try again: {}", err);
                    i += 1;
                    if i > 10 {
                        tracing::error!("Failed to remove container: {}", err);
                        break;
                    }

                    continue;
                }
                break;
            }
        }

        if let Some(web) = web {
            config.cmd = Some(web.split(' ').map(|s| s.to_string()).collect());
        }
    }

    let res = docker
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

    docker
        .start_container(container_name, None::<StartContainerOptions<&str>>)
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

    let network_container = network_inspect
        .containers
        .unwrap_or_default()
        .get(&res.id)
        .unwrap()
        .clone();

    // TODO: this network if for one block. We need to makesure that we can get the right ip
    // attached to the container
    let NetworkContainer {
        ipv4_address,
        ipv6_address,
        ..
    } = network_container;

    tracing::info!(ipv4_address = ?ipv4_address, ipv6_address = ?ipv6_address, "Container {} ip addresses", container_name);

    // TODO: make this configurable
    let ip = ipv6_address
        .filter(|ip| !ip.is_empty())
        .or(ipv4_address.filter(|ip| !ip.is_empty()))
        .and_then(|ip| ip.split('/').next().map(|ip| ip.to_string()))
        .ok_or_else(|| {
            tracing::error!("No ip address found for container {}", container_name);
            anyhow::anyhow!("No ip address found for container {}", container_name)
        })?;

    tracing::info!(ip = ?ip, port = ?port, "Container {} ip address", container_name);

    Ok(DockerContainer {
        ip,
        port,
        build_log,
        db_url,
    })
}
