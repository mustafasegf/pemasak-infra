use std::{
    collections::{HashSet, VecDeque},
    hash::Hash,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use sqlx::PgPool;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::Mutex;
use ulid::Ulid;
use uuid::Uuid;

use crate::docker::build_docker;

type ConcurrentMutex<T> = Arc<Mutex<T>>;

#[derive(Debug)]
pub struct BuildItem {
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
    pub waiting_set: ConcurrentMutex<HashSet<BuildItem>>,
    pub receive_channel: Receiver<(String, String, String, String)>,
    pub pg_pool: PgPool,
}

impl BuildQueue {
    pub fn new(
        build_count: usize,
        pg_pool: PgPool,
    ) -> (Self, Sender<(String, String, String, String)>) {
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
        owner,
        repo,
        container_src,
        container_name,
    }: BuildItem,
    pool: PgPool,
) {
    // TODO: need to emmit error somewhere

    let (ip, port) = match build_docker(&container_name, &container_src).await {
        Ok(ip) => ip,
        Err(err) => {
            tracing::error!(?err, "cannot build container {}", container_name);
            return;
        }
    };

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
        Ok(Some(project)) => project,
        Err(err) => {
            tracing::error!(?err, "Can't get project: Failed to query database");
            return;
        }
        Ok(None) => {
            tracing::error!(container_name, "Can't get project: Project not found");
            return;
        }
    };

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
        Ok(Some(subdomain)) => subdomain.name,
        Err(err) => {
            tracing::error!(?err, "Can't get domain: Failed to query database");
            return;
        }
        Ok(None) => {
            // create domain
            // TODO: clean up this mess
            let subdomain = format!("{owner}-{repo}");
            let id = Uuid::from(Ulid::new());
            if let Err(err) = sqlx::query!(
                r#"INSERT INTO domains (id, project_id, name, port, docker_ip)
                   VALUES ($1, $2, $3, $4, $5)
                "#,
                id,
                project.id,
                subdomain,
                port,
                ip,
            )
            .execute(&pool)
            .await
            {
                tracing::error!(?err, "Can't insert domain: Failed to query database");
                return;
            }
            subdomain
        }
    };

    tracing::debug!("container run on {subdomain}");
}

pub async fn process_task_poll(
    waiting_queue: ConcurrentMutex<VecDeque<BuildItem>>,
    waiting_set: ConcurrentMutex<HashSet<BuildItem>>,
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
            waiting_set.remove(&build_item);

            {
                let build_count = Arc::clone(&build_count);
                let pool = pool.clone();

                build_count.fetch_sub(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    trigger_build(build_item, pool).await;
                    build_count.fetch_add(1, Ordering::SeqCst);
                });
            }
        }
    }
}

pub async fn process_task_enqueue(
    waiting_queue: ConcurrentMutex<VecDeque<BuildItem>>,
    waiting_set: ConcurrentMutex<HashSet<BuildItem>>,
    mut receive_channel: Receiver<(String, String, String, String)>,
) {
    while let Some(message) = receive_channel.recv().await {
        let (container_name, container_src, owner, repo) = message;
        let mut waiting_queue = waiting_queue.lock().await;
        let waiting_set = waiting_set.lock().await;

        let build_item = BuildItem {
            container_name,
            container_src,
            owner,
            repo,
        };

        if waiting_set.contains(&build_item) {
            continue;
        }

        waiting_queue.push_back(build_item);
    }
}

pub async fn build_queue_handler(build_queue: BuildQueue) {
    {
        let waiting_queue = Arc::clone(&build_queue.waiting_queue);
        let waiting_set = Arc::clone(&build_queue.waiting_set);

        tokio::spawn(async move {
            process_task_poll(
                waiting_queue,
                waiting_set,
                build_queue.build_count,
                build_queue.pg_pool,
            )
            .await;
        });
    }
    {
        let waiting_queue = Arc::clone(&build_queue.waiting_queue);
        let waiting_set = Arc::clone(&build_queue.waiting_set);

        tokio::spawn(async move {
            process_task_enqueue(waiting_queue, waiting_set, build_queue.receive_channel).await;
        });
    }
}
