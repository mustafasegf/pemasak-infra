use std::{sync::{atomic::{AtomicUsize, Ordering}, Arc,}, collections::VecDeque, hash::Hash};

use sqlx::PgPool;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::Mutex;
use uuid::Uuid;
use ulid::Ulid;

use crate::docker::build_docker;

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
    pub waiting_queue: Arc<Mutex<VecDeque<BuildItem>>>,
    pub receive_channel: Receiver<(String, String, String, String)>,
    pub pg_pool: PgPool,
}

impl BuildQueue {
    pub fn new(build_count: usize, pg_pool: PgPool) -> (Self, Sender<(String, String, String, String)>) {
        let (tx, rx) = mpsc::channel(32);
        
        (Self {
            build_count: Arc::new(AtomicUsize::new(build_count)),
            waiting_queue: Arc::new(Mutex::new(VecDeque::new())),
            receive_channel: rx,
            pg_pool,
        }, tx)
    }
}

pub async fn process_task_poll(waiting_queue: Arc<Mutex<VecDeque<BuildItem>>>, build_count: Arc<AtomicUsize>, pool: PgPool) {
    loop {
        let mut waiting_queue = waiting_queue.lock().await;
        let bc1 = Arc::clone(&build_count);
        let bc2 = Arc::clone(&build_count);

        if bc1.clone().load(Ordering::SeqCst) > 0 && waiting_queue.len() > 0 {
            println!("BUILD TRIGGERED");
            let build_item = waiting_queue.pop_front();
            let pool = pool.clone();

            bc1.fetch_sub(1, Ordering::SeqCst);

            tokio::spawn(async move {
                println!("BUILD CALLED");

                let build_item = match build_item {
                    Some(build_item) => build_item,
                    None => {
                        bc2.fetch_add(1, Ordering::SeqCst);
                        println!("NO BUILD ITEM");
                        return;
                    },
                };

                let owner = &build_item.owner;
                let repo = &build_item.repo;

                let ip = match build_docker(&build_item.container_name, &build_item.container_src).await {
                    Ok(ip) => ip,
                    Err(err) => {
                        println!("error -> {:#?}", err);
                        bc2.fetch_add(1, Ordering::SeqCst);
                        return;
                    },
                };
                let project = match sqlx::query!(
                    r#"SELECT projects.id
                        FROM projects
                        JOIN project_owners ON projects.owner_id = project_owners.id
                        WHERE project_owners.name = $1
                        AND projects.name = $2"#,
                    owner,
                    repo
                )
                .fetch_optional(&pool)
                .await
                {
                    Ok(Some(project)) => project,
                    Err(err) => {
                        tracing::error!("failed to query database {}", err);
                        bc2.fetch_add(1, Ordering::SeqCst);
                        return;
                    },
                    Ok(None) => {
                        bc2.fetch_add(1, Ordering::SeqCst);
                        return;
                    }
                };

                let port: i32 = 80;

                let subdomain = match sqlx::query!(
                    r#"SELECT domains.name
                       FROM domains
                       WHERE domains.project_id = $1"#,
                    project.id
                )
                .fetch_optional(&pool)
                .await
                {
                    Ok(Some(subdomain)) => subdomain.name,
                    Err(err) => {
                        tracing::error!("failed to query database {}", err);
                        bc2.fetch_add(1, Ordering::SeqCst);
                        return;
                    }
                    Ok(None) => {
                        // create domain
                        // TODO: clean up this mess
                        let subdomain = format!("{owner}-{repo}");
                        let id = Uuid::from(Ulid::new());
                        if let Err(err) = sqlx::query!(
                            r#"INSERT INTO domains (id, project_id, name, port, docker_ip)
                               VALUES ($1, $2, $3, $4, $5)"#,
                            id,
                            project.id,
                            subdomain,
                            port,
                            ip,
                        )
                        .execute(&pool)
                        .await
                        {
                            tracing::error!("failed to query database {}", err);
                            bc2.fetch_add(1, Ordering::SeqCst);
                            return;
                        }
                        subdomain
                    }
                };

                println!("container run on {subdomain}");

                bc2.fetch_add(1, Ordering::SeqCst);
                println!("BUILD FINISHED");
            });
        }
    }
}

pub async fn process_task_enqueue(waiting_queue: Arc<Mutex<VecDeque<BuildItem>>>, mut receive_channel: Receiver<(String, String, String, String)>) {
    while let Some(message) = receive_channel.recv().await {
        let (container_name, container_src, owner, repo) = message;
        let mut waiting_queue = waiting_queue.lock().await;
        waiting_queue.push_back(BuildItem { container_name, container_src, owner, repo });
    }
}

pub async fn build_queue_handler(build_queue: BuildQueue) {
    let queue_handle = build_queue.waiting_queue;

    let qh1 = Arc::clone(&queue_handle);
    let qh2 = Arc::clone(&queue_handle);

    tokio::spawn(async move {
        process_task_poll(qh1, build_queue.build_count, build_queue.pg_pool).await;
    });

    tokio::spawn(async move {
       process_task_enqueue(qh2, build_queue.receive_channel).await;
    });
}
