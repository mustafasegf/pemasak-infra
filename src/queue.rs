use std::borrow::Cow;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use tokio::sync::mpsc::{self, Receiver, Sender};
use ulid::Ulid;
use uuid::Uuid;

use pgmq::PGMQueueExt;

use crate::docker::{build_docker, DockerContainer};

#[derive(Error, Debug)]
#[error("{message:?}")]
pub struct BuildError {
    message: String,
    inner_error: Option<Box<dyn std::error::Error>>,
}
#[derive(Debug)]
pub struct BuildQueueItem {
    pub container_name: String,
    pub container_src: String,
    pub owner: String,
    pub repo: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BuildItem {
    pub build_id: Uuid,
    pub container_name: String,
    pub container_src: String,
    pub owner: String,
    pub repo: String,
}

pub struct BuildQueue {
    pub build_count: Arc<AtomicUsize>,
    pub receive_channel: Receiver<BuildQueueItem>,
    pub pg_pool: PgPool,
}

impl BuildQueue {
    pub fn new(build_count: usize, pg_pool: PgPool) -> (Self, Sender<BuildQueueItem>) {
        let (tx, rx) = mpsc::channel(32);

        (
            Self {
                build_count: Arc::new(AtomicUsize::new(build_count)),
                receive_channel: rx,
                pg_pool,
            },
            tx,
        )
    }
}

pub async fn trigger_build(
    BuildItem {
        build_id,
        owner,
        repo,
        container_src,
        container_name,
    }: BuildItem,
    pool: PgPool,
    host_ip: &str,
) -> Result<String, BuildError> {
    // TODO: need to emmit error somewhere
    let project = match sqlx::query!(
        r#"SELECT projects.id
           FROM projects
           JOIN project_owners ON projects.owner_id = project_owners.id
           WHERE project_owners.name = $1
           AND projects.name = $2
        "#,
        owner,
        repo
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(project) => match project {
            Some(project) => Ok(project),
            None => Err(BuildError {
                message: format!("Project not found with owner {owner} and repo {repo}"),
                inner_error: None,
            }),
        },
        Err(err) => Err(BuildError {
            message: "Can't get project: Failed to query database".to_string(),
            inner_error: Some(err.into()),
        }),
    }?;

    let build_id = match sqlx::query!(
        r#"SELECT builds.id
           FROM builds
           WHERE builds.id = $1
        "#,
        build_id,
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(build)) => Ok(build.id),
        Ok(None) => Err(BuildError {
            message: format!("Failed to find build with id: {build_id}"),
            inner_error: None,
        }),
        Err(err) => Err(BuildError {
            message: "Can't create build: Failed to query database".to_string(),
            inner_error: Some(err.into()),
        }),
    }?;

    if let Err(err) = sqlx::query!(
        "UPDATE builds set status = 'building' where id = $1",
        build_id
    )
    .execute(&pool)
    .await
    {
        return Err(BuildError {
            message: "Failed to update build status: Failed to query database".to_string(),
            inner_error: Some(err.into()),
        });
    }

    // TODO: Differentiate types of errors returned by build_docker (ex: ImageBuildError, NetworkCreateError, ContainerAttachError)
    let DockerContainer {
        ip,
        port,
        db_url,
        subnet,
        ..
    } = match build_docker(&repo, &container_name, &container_src, pool.clone()).await {
        Ok(result) => {
            if let Err(err) = sqlx::query!(
                "UPDATE builds SET status = 'successful', log = $1 WHERE id = $2",
                result.build_log,
                build_id
            )
            .execute(&pool)
            .await
            {
                return Err(BuildError {
                    message: "Failed to update build status: Failed to query database".to_string(),
                    inner_error: Some(err.into()),
                });
            }

            if let Err(err) = sqlx::query!(
                "UPDATE projects SET state = 'running' WHERE id = $1",
                project.id
            )
            .execute(&pool)
            .await
            {
                return Err(BuildError {
                    message: "Failed to update project state: Failed to query database".to_string(),
                    inner_error: Some(err.into()),
                });
            }

            Ok(result)
        }
        Err(err) => {
            if let Err(err) = sqlx::query!(
                "UPDATE builds SET status = 'failed', log = $1 WHERE id = $2",
                err.to_string(),
                build_id
            )
            .execute(&pool)
            .await
            {
                return Err(BuildError {
                    message: format!(
                        "Failed to update build status: Failed to query database: {repo}"
                    ),
                    inner_error: Some(err.into()),
                });
            }

            return Err(BuildError {
                message: format!("A build error occured while building repository: {repo}"),
                inner_error: Some(err.into()),
            });
        }
    }?;

    // TODO: check why why need this
    let subdomain = match sqlx::query!(
        r#"SELECT domains.name, domains.subnet, domains.host_ip
           FROM domains
           WHERE domains.project_id = $1
        "#,
        project.id
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(subdomain)) => {
            if subdomain.subnet == "0.0.0.0/0" {
                sqlx::query!(
                    "UPDATE domains SET subnet = $1 WHERE project_id = $2",
                    subnet,
                    project.id
                )
                .execute(&pool)
                .await
                .map_err(|err| BuildError {
                    message: "Failed to update domain subnet: Failed to query database".to_string(),
                    inner_error: Some(err.into()),
                })?;
            }

            if subdomain.host_ip == "0.0.0.0" {
                sqlx::query!(
                    "UPDATE domains SET host_ip = $1 WHERE project_id = $2",
                    host_ip,
                    project.id
                )
                .execute(&pool)
                .await
                .map_err(|err| BuildError {
                    message: "Failed to update domain host_ip: Failed to query database"
                        .to_string(),
                    inner_error: Some(err.into()),
                })?;
            }
            Ok(subdomain.name)
        }
        Ok(None) => {
            let id = Uuid::from(Ulid::new());
            let subdomain = sqlx::query!(
                r#"INSERT INTO domains (id, project_id, name, port, docker_ip, db_url, subnet, host_ip)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
                id,
                project.id,
                container_name,
                port,
                ip,
                db_url,
                subnet,
                host_ip,
            )
            .execute(&pool)
            .await;

            match subdomain {
                Ok(_) => Ok(container_name),
                Err(err) => Err(BuildError {
                    inner_error: Some(err.into()),
                    message: "Can't insert domain: Failed to query database".to_string(),
                }),
            }
        }
        Err(err) => Err(BuildError {
            message: "Can't get subdomain: Failed to query database".to_string(),
            inner_error: Some(err.into()),
        }),
    }?;

    Ok(subdomain)
}

pub async fn process_task_poll(
    build_count: Arc<AtomicUsize>,
    pool: PgPool,
    idle_channel: Sender<String>,
    host_ip: String,
) {
    let host_ip: Cow<'_, str> = Cow::Owned(host_ip);
    let queue = PGMQueueExt::new_with_pool(pool.clone()).await.unwrap();

    loop {
        let host_ip = host_ip.clone();
        let build_count = Arc::clone(&build_count);

        // TODO: handle error
        let waiting_queue_count = sqlx::query!(
            r#"SELECT COUNT(*) as count
               FROM pgmq.q_build_queue
            "#
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .count
        .unwrap();

        if build_count.load(Ordering::SeqCst) > 0 && waiting_queue_count > 0 {
            let build_item = match queue.pop::<BuildItem>("build_queue").await {
                Ok(Some(build_item)) => build_item,
                Ok(None) => continue,
                Err(err) => {
                    tracing::error!(?err, "Failed to pop from queue");
                    continue;
                }
            };

            let container_name = build_item.message.container_name.clone();
            let idle_channel = idle_channel.clone();
            if let Err(err) = idle_channel.send(container_name.clone()).await {
                tracing::error!(?err, "Failed to send idle message");
            };

            {
                let build_count = Arc::clone(&build_count);
                let pool = pool.clone();

                build_count.fetch_sub(1, Ordering::SeqCst);

                tokio::spawn(async move {
                    match trigger_build(build_item.message, pool, &host_ip).await {
                        Ok(subdomain) => tracing::info!("Project deployed at {subdomain}"),
                        Err(BuildError {
                            message,
                            inner_error,
                        }) => tracing::error!(err = ?inner_error, message),
                    };

                    if let Err(err) = idle_channel.send(container_name.clone()).await {
                        tracing::error!(?err, "Failed to send idle message");
                    };

                    build_count.fetch_add(1, Ordering::SeqCst);
                });
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

pub async fn process_task_enqueue(pool: PgPool, mut receive_channel: Receiver<BuildQueueItem>) {
    let queue = PGMQueueExt::new_with_pool(pool.clone()).await.unwrap();
    while let Some(message) = receive_channel.recv().await {
        let BuildQueueItem {
            container_name,
            container_src,
            owner,
            repo,
        } = message;

        let project = match sqlx::query!(
            r#"SELECT projects.id
               FROM projects
               JOIN project_owners ON projects.owner_id = project_owners.id
               WHERE project_owners.name = $1
               AND projects.name = $2
            "#,
            owner,
            repo
        )
        .fetch_optional(&pool)
        .await
        {
            Ok(project) => match project {
                Some(project) => project,
                None => {
                    tracing::error!("Project not found with owner {} and repo {}", owner, repo);
                    continue;
                }
            },
            Err(err) => {
                tracing::error!(?err, "Can't query project: Failed to query database");
                continue;
            }
        };

        // TODO: change into transactional check

        match sqlx::query!(
            r#"
                  SELECT *
                  FROM pgmq.q_build_queue
                  WHERE MESSAGE ->> 'container_name' = $1
            "#,
            container_name
        )
        .fetch_optional(&pool)
        .await
        {
            Ok(None) => {}
            Ok(Some(_)) => {
                tracing::info!(container_name, "Container already in queue");
                continue;
            }
            Err(err) => {
                tracing::error!(
                    ?err,
                    container_name,
                    "Can't query queue: Failed to query database"
                );
                continue;
            }
        };

        let build_id = Uuid::from(Ulid::new());
        if let Err(err) = sqlx::query!(
            r#"INSERT INTO builds (id, project_id)
               VALUES ($1, $2)
            "#,
            build_id,
            project.id,
        )
        .execute(&pool)
        .await
        {
            tracing::error!(?err, "Can't create build: Failed to query database");
            continue;
        };

        let build_item = BuildItem {
            build_id,
            container_name,
            container_src,
            owner,
            repo,
        };

        queue.send("build_queue", &build_item).await.unwrap();
    }
}

pub async fn build_queue_handler(
    build_queue: BuildQueue,
    idle_channel: Sender<String>,
    host_ip: String,
) {
    {
        let pool = build_queue.pg_pool.clone();

        tokio::spawn(async move {
            process_task_poll(build_queue.build_count, pool, idle_channel, host_ip).await;
        });
    }
    {
        let pool = build_queue.pg_pool.clone();

        tokio::spawn(async move {
            process_task_enqueue(pool, build_queue.receive_channel).await;
        });
    }
}
