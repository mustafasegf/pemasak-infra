use std::process::Output;
use std::str::FromStr;
use std::time::Duration;
use std::{collections::HashMap, process::Stdio};

use anyhow::Result;
use bollard::container::WaitContainerOptions;
use bollard::network::CreateNetworkOptions;
use bollard::service::{
    EndpointIpamConfig, EndpointSettings, HealthConfig, HealthStatusEnum, Network,
};
use bollard::{
    container::{Config, CreateContainerOptions, StartContainerOptions},
    image::TagImageOptions,
    network::{ConnectNetworkOptions, InspectNetworkOptions},
    service::{HostConfig, NetworkContainer, RestartPolicy, RestartPolicyNameEnum},
    volume::CreateVolumeOptions,
    Docker,
};
use futures_util::TryStreamExt;
use nixpacks::{
    create_docker_image,
    nixpacks::{builder::docker::DockerBuilderOptions, plan::generator::GeneratePlanOptions},
};
use procfile;
use rand::{Rng, SeedableRng};
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::process::Command;

const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub struct DockerContainer {
    pub ip: String,
    pub port: i32,
    pub build_log: String,
    pub db_url: String,
    pub subnet: String,
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
        tracing::error!(?err, "Failed to connect to docker: {}", err);
        err
    })?;

    remove_old_image(&docker, &image_name, container_name).await?;

    tracing::info!("Start building {}", container_name);

    let (build_log, nixpacks) = match std::path::Path::new(container_src)
        .join("Dockerfile")
        .exists()
    {
        true => {
            tracing::debug!(container_name, "Build using dockerfile");
            build_dockerfile(container_src, &image_name).await?
        }
        false => {
            tracing::debug!(container_name, "Build using nixpacks");

            let plan_options = GeneratePlanOptions::default();
            let build_options = DockerBuilderOptions {
                name: Some(container_name.to_string()),
                quiet: false,
                verbose: true,
                ..Default::default()
            };
            let envs = vec![];

            let Output {
                status,
                stderr,
                stdout: _,
            } = create_docker_image(container_src, envs, &plan_options, &build_options).await?;

            let build_log = String::from_utf8(stderr).unwrap();

            if !status.success() {
                return Err(anyhow::anyhow!(build_log));
            }
            (build_log, true)
        }
    };

    docker.inspect_image(&image_name).await?;

    match docker.inspect_container(container_name, None).await {
        Ok(_) => {
            remove_container(&docker, container_name).await?;

            docker
                .remove_image(&old_image_name, None, None)
                .await
                .map_err(|err| {
                    tracing::error!(?err, "Failed to remove image: {}", err);
                    err
                })?;
        }
        Err(bollard::errors::Error::DockerResponseServerError { .. }) => {}
        Err(err) => {
            tracing::error!(?err, "Failed to inspect container: {}", err);
            return Err(err.into());
        }
    }

    let db_container_exist = match docker.inspect_container(&db_name, None).await {
        Err(bollard::errors::Error::DockerResponseServerError { .. }) => false,
        Err(err) => {
            tracing::error!(?err, "Failed to inspect container: {}", err);
            return Err(err.into());
        }
        _ => true,
    };

    match docker.inspect_volume(&volume_name).await {
        Err(bollard::errors::Error::DockerResponseServerError { .. }) => {
            docker
                .create_volume(CreateVolumeOptions {
                    name: volume_name.clone(),
                    ..Default::default()
                })
                .await
                .map_err(|err| {
                    tracing::error!(?err, "Failed to create volume: {}", err);
                    err
                })?;
        }
        Err(err) => {
            tracing::error!(?err, "Failed to inspect volume: {}", err);
            return Err(err.into());
        }
        _ => {}
    };

    let network = create_network(&docker, pool.clone(), container_name, &network_name).await?;
    // TODO: it would be nice if we don't use unwrap
    let subnet = network
        .ipam
        .and_then(|ipam| ipam.config)
        .and_then(|config| config.into_iter().next())
        .and_then(|config| config.subnet.clone())
        .ok_or_else(|| {
            tracing::error!("No subnet found for network {}", network_name);
            anyhow::anyhow!("No subnet found for network {}", network_name)
        })?;

    let mut ip_range = ipnet::Ipv4Net::from_str(&subnet).unwrap().hosts();
    let _gateway = ip_range.next();

    let container_ip = ip_range.next().unwrap().to_string();

    // create database container if it doesn't exist
    let db_url = match db_container_exist {
        false => {
            create_db(
                &docker,
                &db_name,
                &volume_name,
                &network_name,
                &mut ip_range,
            )
            .await?
        }
        true => {
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
                    // delete database and create again
                    tracing::debug!("No database url found for project {}", project_name);

                    remove_container(&docker, &db_name).await?;
                    remove_volume(&docker, &volume_name).await?;
                    create_db(
                        &docker,
                        &db_name,
                        &volume_name,
                        &network_name,
                        &mut ip_range,
                    )
                    .await?
                }
                Err(err) => {
                    tracing::error!(?err, "Failed to query database: {}", err);
                    return anyhow::Result::Err(err.into());
                }
            }
        }
    };

    // TODO: figure out if we need make this configurable
    let port = 80;

    let mut envs = sqlx::query!(
        r#"SELECT envs FROM projects WHERE projects.name = $1"#,
        project_name
    )
    .fetch_one(&pool)
    .await
    .map_err(|err| {
        tracing::error!(?err, "Failed to query database: {}", err);
        err
    })
    .map(|row| row.envs)?;

    envs["DATABASE_URL"] = json!(db_url);
    envs["PORT"] = json!(port);

    // flatten to vec
    let envs = envs
        .as_object()
        .unwrap()
        .into_iter()
        .map(|(key, value)| {
            let value = match value {
                Value::String(value) => value.to_owned(),
                Value::Number(value) => value.to_string(),
                Value::Bool(value) => value.to_string(),
                _ => String::new(),
            };
            format!("{}={}", key, value)
        })
        .collect::<Vec<_>>();

    let mut config = Config {
        image: Some(image_name.clone()),
        // TDDO: rethink if we need to make this configurable
        env: Some(envs),
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
                            tracing::error!(?err, "Failed to parse Procfile: {}", err);
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
            let container_release_name = format!("{}-release", container_name);
            let mut config = config.clone();
            config.host_config = Some(HostConfig {
                restart_policy: Some(RestartPolicy {
                    name: Some(RestartPolicyNameEnum::NO),
                    ..Default::default()
                }),
                ..Default::default()
            });

            config.cmd = Some(release.split(' ').map(|s| s.to_string()).collect());

            if let Err(err) = docker
                .create_container(
                    Some(CreateContainerOptions {
                        name: &container_release_name,
                        platform: None,
                    }),
                    config,
                )
                .await
            {
                tracing::error!(?err, "Failed to create container: {}", err);
                if !db_container_exist {
                    remove_container(&docker, &db_name).await?;
                    remove_volume(&docker, &volume_name).await?;
                }

                return Err(err.into());
            }

            docker
                .connect_network(
                    &network_name,
                    ConnectNetworkOptions {
                        container: container_release_name.clone(),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|err| {
                    tracing::error!(?err, "Failed to connect network: {}", err);
                    err
                })?;

            if let Err(err) = docker
                .start_container(&container_release_name, None::<StartContainerOptions<&str>>)
                .await
            {
                tracing::error!(?err, "Failed to start container: {}", err);

                if !db_container_exist {
                    remove_container(&docker, &db_name).await?;
                    remove_volume(&docker, &volume_name).await?;
                }
            }

            remove_container(&docker, &container_release_name)
                .await
                .map_err(|err| {
                    tracing::error!(?err, "Failed to remove container: {}", err);
                    err
                })?;
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
            tracing::error!(?err, "Failed to create container: {}", err);
            err
        })?;

    // connect container to network
    docker
        .connect_network(
            &network_name,
            ConnectNetworkOptions {
                container: container_name.clone(),
                endpoint_config: EndpointSettings {
                    ipam_config: Some(EndpointIpamConfig {
                        ipv4_address: Some(container_ip),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            },
        )
        .await
        .map_err(|err| {
            tracing::error!(?err, "Failed to connect network: {}", err);
            err
        })?;

    docker
        .start_container(container_name, None::<StartContainerOptions<&str>>)
        .await
        .map_err(|err| {
            tracing::error!(?err, "Failed to start container: {}", err);
            err
        })?;

    //inspect network
    let network_inspect = docker
        .inspect_network(
            &network_name,
            Some(InspectNetworkOptions::<&str> {
                verbose: true,
                ..Default::default()
            }),
        )
        .await
        .map_err(|err| {
            tracing::error!(?err, "Failed to inspect network: {}", err);
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
        subnet,
    })
}

async fn remove_old_image(
    docker: &Docker,
    container_name: &str,
    image_name: &str,
) -> Result<(), bollard::errors::Error> {
    match docker.inspect_image(image_name).await {
        Ok(_) => {
            let tag_options = TagImageOptions {
                tag: "old",
                repo: container_name,
            };

            docker
                .tag_image(container_name, Some(tag_options))
                .await
                .map_err(|err| {
                    tracing::error!(?err, "Failed to tag image: {}", err);
                    err
                })?;

            docker
                .remove_image(image_name, None, None)
                .await
                .map_err(|err| {
                    tracing::error!(?err, "Failed to remove image: {}", err);
                    err
                })?;
        }
        Err(bollard::errors::Error::DockerResponseServerError { .. }) => {}
        Err(err) => {
            tracing::error!(?err, "Failed to inspect image: {}", err);
            return Err(err);
        }
    };
    Ok(())
}

pub async fn build_dockerfile(container_src: &str, image_name: &str) -> Result<(String, bool)> {
    // build from Dockerfile
    let mut cmd = Command::new("docker");
    cmd.args([
        "build",
        "-t",
        image_name,
        "-f",
        (std::path::Path::new(container_src)
            .join("Dockerfile")
            .to_str()
            .unwrap()),
        container_src,
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let child = cmd.spawn().map_err(|err| {
        tracing::error!(?err, "Failed to spawn docker build: {}", err);
        err
    })?;

    let output = child.wait_with_output().await.map_err(|err| {
        tracing::error!(?err, "Failed to wait for docker build: {}", err);
        err
    })?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(String::from_utf8(output.stderr).unwrap()));
    }
    match output.status.success() {
        true => Ok((String::from_utf8(output.stderr).unwrap(), false)),
        false => {
            let err = anyhow::anyhow!(
                "Failed to build image: {}",
                String::from_utf8(output.stderr).unwrap()
            );
            tracing::error!(?err, "Failed to build image");

            Err(err)
        }
    }
}

pub async fn remove_container(
    docker: &Docker,
    container_name: &str,
) -> Result<(), bollard::errors::Error> {
    docker
        .stop_container(container_name, None)
        .await
        .map_err(|err| {
            tracing::error!(?err, "Failed to stop container: {}", err);
            err
        })?;

    if let Err(err) = docker
        .wait_container(container_name, None::<WaitContainerOptions<&str>>)
        .try_collect::<Vec<_>>()
        .await
    {
        tracing::warn!(
            ?err,
            "Container {} Stoped Have Error: {}",
            container_name,
            err
        );
    }

    docker
        .remove_container(container_name, None)
        .await
        .map_err(|err| {
            tracing::error!(?err, "Failed to remove container: {}", err);
            err
        })?;
    Ok(())
}

pub async fn remove_volume(
    docker: &Docker,
    volume_name: &str,
) -> Result<(), bollard::errors::Error> {
    docker
        .remove_volume(volume_name, None)
        .await
        .map_err(|err| {
            tracing::error!(?err, "Failed to remove volume: {}", err);
            err
        })?;
    Ok(())
}

pub async fn create_network(
    docker: &Docker,
    pool: PgPool,
    container_name: &str,
    network_name: &str,
) -> Result<Network> {
    // check if network exists
    let network = docker
        .inspect_network(
            &network_name,
            Some(InspectNetworkOptions::<&str> {
                verbose: true,
                ..Default::default()
            }),
        )
        .await;

    // create network if it doesn't exist
    match network {
        Ok(network) => {
            tracing::info!(id = ?network.id, "Use existing network id {:?}", network.id);

            // check if subnet is predefined
            match sqlx::query!(
                r#"select subnet from domains where name = $1 and subnet = '0.0.0.0/0' "#,
                container_name,
            )
            .fetch_optional(&pool)
            .await
            {
                Ok(None) => {}
                Ok(Some(_)) => {
                    tracing::debug!("Subnet is not predefined");

                    let containers = network
                        .clone()
                        .containers
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(_, container)| container)
                        .collect::<Vec<_>>();

                    for container in containers.iter() {
                        docker
                            .disconnect_network(
                                &network_name,
                                bollard::network::DisconnectNetworkOptions {
                                    container: container.name.as_ref().unwrap(),
                                    force: true,
                                },
                            )
                            .await
                            .map_err(|err| {
                                tracing::error!(?err, "Failed to disconnect network: {}", err);
                                err
                            })?;
                    }

                    docker.remove_network(&network_name).await.map_err(|err| {
                        tracing::error!(?err, "Failed to remove network: {}", err);
                        err
                    })?;

                    let subnet = network
                        .clone()
                        .ipam
                        .and_then(|ipam| ipam.config)
                        .and_then(|configs| configs.into_iter().next())
                        .and_then(|config| config.subnet.clone())
                        .ok_or_else(|| {
                            tracing::error!("No subnet found for network {}", network_name);
                            anyhow::anyhow!("No subnet found for network {}", network_name)
                        })?;

                    let subnet_string = subnet.to_string();
                    let gateway = ipnet::Ipv4Net::from_str(&subnet_string)
                        .unwrap()
                        .hosts()
                        .next()
                        .unwrap()
                        .to_string();
                    let mut ip_range = ipnet::Ipv4Net::from_str(&subnet_string).unwrap().hosts();

                    // remove gateway
                    ip_range.next();

                    docker
                        .create_network(CreateNetworkOptions {
                            name: network_name.clone(),
                            ipam: bollard::models::Ipam {
                                config: Some(vec![bollard::models::IpamConfig {
                                    subnet: Some(subnet_string),
                                    gateway: Some(gateway),
                                    ..Default::default()
                                }]),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .await
                        .map_err(|err| {
                            tracing::error!(?err, "Failed to create network: {}", err);
                            err
                        })?;

                    let new_network = docker
                        .inspect_network(
                            &network_name,
                            Some(InspectNetworkOptions::<&str> {
                                verbose: true,
                                ..Default::default()
                            }),
                        )
                        .await
                        .map_err(|err| {
                            tracing::error!(?err, "Failed to inspect network: {}", err);
                            err
                        })?;

                    let new_subnet = new_network
                        .clone()
                        .ipam
                        .and_then(|ipam| ipam.config)
                        .and_then(|configs| configs.into_iter().next())
                        .and_then(|config| config.subnet.clone())
                        .ok_or_else(|| {
                            tracing::error!("No subnet found for network {}", network_name);
                            anyhow::anyhow!("No subnet found for network {}", network_name)
                        })?;

                    // make sure the container we want to connect is the first one

                    let index = containers
                        .iter()
                        .enumerate()
                        .find(|(_, &ref e)| e.name.as_ref().unwrap() == container_name)
                        .map(|(index, _)| index)
                        .unwrap_or_default();

                    let containers = match index {
                        0 => containers,
                        index => containers[index..=index]
                            .iter()
                            .cloned()
                            .chain(containers[..index].iter().cloned())
                            .chain(containers[index + 1..].iter().cloned())
                            .collect::<Vec<_>>(),
                    };

                    for container in containers {
                        docker
                            .connect_network(
                                &network_name,
                                ConnectNetworkOptions {
                                    container: container.name.unwrap(),
                                    endpoint_config: EndpointSettings {
                                        ipam_config: Some(EndpointIpamConfig {
                                            ipv4_address: Some(container.ipv4_address.unwrap()),
                                            ..Default::default()
                                        }),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                },
                            )
                            .await
                            .map_err(|err| {
                                tracing::error!(?err, "Failed to connect network: {}", err);
                                err
                            })?;
                    }
                }
                Err(err) => {
                    tracing::error!(?err, "Failed to query database: {}", err);
                    return Err(err.into());
                }
            }
            Ok(network)
        }
        Err(_) => {
            loop {
                docker
                    .create_network(CreateNetworkOptions {
                        name: network_name.clone(),
                        ..Default::default()
                    })
                    .await
                    .map_err(|err| {
                        tracing::error!(?err, "Failed to create network: {}", err);
                        err
                    })?;

                let network = docker
                    .inspect_network(
                        &network_name,
                        Some(InspectNetworkOptions::<&str> {
                            verbose: true,
                            ..Default::default()
                        }),
                    )
                    .await
                    .map_err(|err| {
                        tracing::error!(?err, "Failed to inspect network: {}", err);
                        err
                    })?;

                let subnet = network
                    .clone()
                    .ipam
                    .and_then(|ipam| ipam.config)
                    .and_then(|configs| configs.into_iter().next())
                    .and_then(|config| config.subnet.clone())
                    .ok_or_else(|| {
                        tracing::error!("No subnet found for network {}", network_name);
                        anyhow::anyhow!("No subnet found for network {}", network_name)
                    })?;

                let subnet_string = subnet.to_string();

                let gateway = ipnet::Ipv4Net::from_str(&subnet_string)
                    .unwrap()
                    .hosts()
                    .next()
                    .unwrap()
                    .to_string();

                // check if subnet is already used in db
                match sqlx::query!(
                    r#"select subnet from domains where subnet = $1"#,
                    subnet_string,
                )
                .fetch_optional(&pool)
                .await
                {
                    Ok(Some(_)) => {
                        tracing::debug!("Subnet {} is already used", subnet_string);
                        std::thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                    Ok(None) => {
                        tracing::debug!("Subnet {} is not used", subnet_string);
                    }
                    Err(err) => {
                        tracing::error!(?err, "Failed to query database: {}", err);
                        return Err(err.into());
                    }
                }

                // delete network and create again with pre-defined subnet
                docker.remove_network(&network_name).await.map_err(|err| {
                    tracing::error!(?err, "Failed to remove network: {}", err);
                    err
                })?;

                docker
                    .create_network(CreateNetworkOptions {
                        name: network_name.clone(),
                        ipam: bollard::models::Ipam {
                            config: Some(vec![bollard::models::IpamConfig {
                                subnet: Some(subnet_string),
                                gateway: Some(gateway),
                                ..Default::default()
                            }]),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .await
                    .map_err(|err| {
                        tracing::error!(?err, "Failed to create network: {}", err);
                        err
                    })?;

                break Ok(network);
            }
        }
    }
}

pub async fn create_db(
    docker: &Docker,
    db_name: &str,
    volume_name: &str,
    network_name: &str,
    ip_range: &mut ipnet::Ipv4AddrRange,
) -> Result<String> {
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
        healthcheck: Some(HealthConfig {
            test: Some(vec![
                "CMD-SHELL".to_string(),
                "pg_isready -U postgres".to_string(),
            ]),
            interval: Some(Duration::from_secs(5).as_nanos() as i64),
            timeout: Some(Duration::from_secs(5).as_nanos() as i64),
            retries: Some(10),
            start_period: Some(Duration::from_secs(5).as_nanos() as i64),
        }),
        ..Default::default()
    };

    docker
        .create_container(
            Some(CreateContainerOptions {
                name: db_name.clone(),
                platform: None,
            }),
            config,
        )
        .await
        .map_err(|err| {
            tracing::error!(?err, "Failed to create container: {}", err);
            err
        })?;

    start_container(&docker, db_name, true)
        .await
        .map_err(|err| {
            tracing::error!(?err, "Failed to start container: {}", err);
            err
        })?;

    let db_ip = ip_range.next().unwrap().to_string();
    // connect db container to network
    docker
        .connect_network(
            network_name,
            ConnectNetworkOptions {
                container: db_name.clone(),
                endpoint_config: EndpointSettings {
                    ipam_config: Some(EndpointIpamConfig {
                        ipv4_address: Some(db_ip.clone()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            },
        )
        .await
        .map_err(|err| {
            tracing::error!(?err, "Failed to connect network: {}", err);
            err
        })?;

    Ok(format!(
        "postgresql://{}:{}@{}:{}/{}",
        username, password, db_name, 5432, "postgres"
    ))
}

pub async fn start_container(
    docker: &Docker,
    container_name: &str,
    db: bool,
) -> Result<(), bollard::errors::Error> {
    docker
        .start_container(container_name, None::<StartContainerOptions<&str>>)
        .await
        .map_err(|err| {
            tracing::error!(?err, "Failed to start container: {}", err);
            err
        })?;

    if db {
        loop {
            std::thread::sleep(Duration::from_secs(2));

            match docker.inspect_container(container_name, None).await {
                Err(err) => {
                    tracing::debug!("Failed to inspect container. Will try again: {}", err);
                    continue;
                }
                Ok(container) => {
                    if container
                        .state
                        .and_then(|state| state.health)
                        .and_then(|health| health.status)
                        .and_then(|status| status.eq(&HealthStatusEnum::HEALTHY).then_some(()))
                        .is_some()
                    {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}
