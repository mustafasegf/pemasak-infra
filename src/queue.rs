use std::{
    collections::{HashSet, VecDeque},
    hash::Hash,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use anyhow::Result;
use sqlx::PgPool;
use thiserror::Error;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use ulid::Ulid;
use uuid::Uuid;

use crate::docker::{build_docker, DockerContainer};

type ConcurrentMutex<T> = Arc<Mutex<T>>;

const TASK_POLL_DELAY: u64 = 100;

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

#[derive(Debug)]
pub struct BuildItem {
    pub build_id: Uuid,
    pub container_name: String,
    pub container_src: String,
    pub owner: String,
    pub repo: String,
}

impl Hash for BuildItem {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.container_name.hash(state)
    }
}

impl PartialEq for BuildItem {
    fn eq(&self, other: &Self) -> bool {
        self.container_name == other.container_name
    }
}

impl Eq for BuildItem {}

pub struct BuildQueue {
    pub build_count: Arc<AtomicUsize>,
    pub waiting_queue: ConcurrentMutex<VecDeque<BuildItem>>,
    pub waiting_set: ConcurrentMutex<HashSet<String>>,
    pub receive_channel: Receiver<BuildQueueItem>,
    pub pg_pool: PgPool,
}

impl BuildQueue {
    pub fn new(build_count: usize, pg_pool: PgPool) -> (Self, Sender<BuildQueueItem>) {
        let (tx, rx) = mpsc::channel(32);

        (
            Self {
                build_count: Arc::new(AtomicUsize::new(build_count)),
                waiting_queue: Arc::new(Mutex::new(VecDeque::new())),
                waiting_set: Arc::new(Mutex::new(HashSet::new())),
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
        ip, port, db_url, ..
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
        r#"SELECT domains.name
           FROM domains
           WHERE domains.project_id = $1
        "#,
        project.id
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(subdomain)) => Ok(subdomain.name),
        Ok(None) => {
            let id = Uuid::from(Ulid::new());
            let subdomain = sqlx::query!(
                r#"INSERT INTO domains (id, project_id, name, port, docker_ip, db_url)
                   VALUES ($1, $2, $3, $4, $5, $6)
                "#,
                id,
                project.id,
                container_name,
                port,
                ip,
                db_url
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
    waiting_queue: ConcurrentMutex<VecDeque<BuildItem>>,
    waiting_set: ConcurrentMutex<HashSet<String>>,
    build_count: Arc<AtomicUsize>,
    pool: PgPool,
) {
    loop {
        let mut waiting_queue = waiting_queue.lock().await;
        let mut waiting_set = waiting_set.lock().await;

        let build_count = Arc::clone(&build_count);

        if build_count.load(Ordering::SeqCst) > 0 && waiting_queue.len() > 0 {
            let build_item = match waiting_queue.pop_front() {
                Some(build_item) => build_item,
                None => continue,
            };
            waiting_set.remove(&build_item.container_name);

            {
                let build_count = Arc::clone(&build_count);
                let pool = pool.clone();

                build_count.fetch_sub(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    match trigger_build(build_item, pool).await {
                        Ok(subdomain) => tracing::info!("Project deployed at {subdomain}"),
                        Err(BuildError {
                            message,
                            inner_error,
                        }) => tracing::error!(?inner_error, message),
                    };

                    build_count.fetch_add(1, Ordering::SeqCst);
                });
            }
        }
        sleep(Duration::from_millis(TASK_POLL_DELAY)).await;
    }
}

pub async fn process_task_enqueue(
    waiting_queue: ConcurrentMutex<VecDeque<BuildItem>>,
    waiting_set: ConcurrentMutex<HashSet<String>>,
    pool: PgPool,
    mut receive_channel: Receiver<BuildQueueItem>,
) {
    while let Some(message) = receive_channel.recv().await {
        let BuildQueueItem {
            container_name,
            container_src,
            owner,
            repo,
        } = message;
        let mut waiting_queue = waiting_queue.lock().await;
        let mut waiting_set = waiting_set.lock().await;

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
                tracing::error!(%err, "Can't query project: Failed to query database");
                continue;
            }
        };

        if waiting_set.contains(&container_name) {
            continue;
        }

        let build_id = Uuid::from(Ulid::new());
        match sqlx::query!(
            r#"INSERT INTO builds (id, project_id)
               VALUES ($1, $2)
            "#,
            build_id,
            project.id,
        )
        .fetch_optional(&pool)
        .await
        {
            Ok(build_details) => build_details,
            Err(err) => {
                tracing::error!(%err, "Can't create build: Failed to query database");
                continue;
            }
        };

        let build_item = BuildItem {
            build_id,
            container_name,
            container_src,
            owner,
            repo,
        };

        waiting_set.insert(build_item.container_name.clone());
        waiting_queue.push_back(build_item);
    }
}

pub async fn build_queue_handler(build_queue: BuildQueue) {
    {
        let waiting_queue = Arc::clone(&build_queue.waiting_queue);
        let waiting_set = Arc::clone(&build_queue.waiting_set);
        let pool = build_queue.pg_pool.clone();

        tokio::spawn(async move {
            process_task_poll(waiting_queue, waiting_set, build_queue.build_count, pool).await;
        });
    }
    {
        let waiting_queue = Arc::clone(&build_queue.waiting_queue);
        let waiting_set = Arc::clone(&build_queue.waiting_set);
        let pool = build_queue.pg_pool.clone();

        tokio::spawn(async move {
            process_task_enqueue(
                waiting_queue,
                waiting_set,
                pool,
                build_queue.receive_channel,
            )
            .await;
        });
    }
}
