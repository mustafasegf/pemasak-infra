use std::{sync::{atomic::{AtomicUsize, Ordering}, Arc, Mutex}, collections::{BTreeSet, HashMap, VecDeque}, hash::Hash};

use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::docker::build_docker;

pub struct BuildItem {
    pub container_name: String,
    pub container_src: String,
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
    pub receive_channel: Receiver<(String, String)>,
}

impl BuildQueue {
    pub fn new(build_count: usize) -> (Self, Sender<(String, String)>) {
        let (tx, rx) = mpsc::channel(32);
        
        (Self {
            build_count: Arc::new(AtomicUsize::new(build_count)),
            waiting_queue: Arc::new(Mutex::new(VecDeque::new())),
            receive_channel: rx,
        }, tx)
    }
}

pub async fn process_task_poll(waiting_queue: Arc<Mutex<VecDeque<BuildItem>>>, build_count: Arc<AtomicUsize>) {
    loop {
        let mut waiting_queue = waiting_queue.lock().unwrap();
        let bc1 = Arc::clone(&build_count);
        let bc2 = Arc::clone(&build_count);

        if bc1.clone().load(Ordering::SeqCst) > 0 {
            let build_item = waiting_queue.pop_front();
            bc1.fetch_sub(1, Ordering::SeqCst);

            tokio::spawn(async move {
                let ip = match build_item {
                    Some(build_item) => Some(build_docker(&build_item.container_name, &build_item.container_src).await),
                    None => Option::None,
                };

                bc2.fetch_add(1, Ordering::SeqCst);
            });
        }
    }
}

pub async fn process_task_enqueue(waiting_queue: Arc<Mutex<VecDeque<BuildItem>>>, mut receive_channel: Receiver<(String, String)>) {
    while let Some(message) = receive_channel.recv().await {
        let (container_name, container_src) = message;
        let mut waiting_queue = waiting_queue.lock().unwrap();
        waiting_queue.push_back(BuildItem { container_name, container_src });
    }
}

pub async fn build_queue_handler(build_queue: BuildQueue) {
    let queue_handle = build_queue.waiting_queue;

    let qh1 = Arc::clone(&queue_handle);
    let qh2 = Arc::clone(&queue_handle);

    tokio::spawn(async move {
        process_task_poll(qh1, build_queue.build_count).await;
    });

    tokio::spawn(async move {
       process_task_enqueue(qh2, build_queue.receive_channel).await;
    });
}
