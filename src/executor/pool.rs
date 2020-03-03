use super::{JoinHandle, Task};
use std::sync::{atomic::Ordering, Arc};
use std::thread::{self, JoinHandle as ThreadHandle};

use super::spawner;
use super::worker;

pub struct ThreadPool {
    spawner: spawner::Spawner,
}

pub struct Workers {
    workers: Vec<worker::Worker>,
}

impl ThreadPool {
    pub fn new() -> (Self, Workers) {
        const N: usize = 4;

        let (shared, workers) = worker::create_set(N);

        let pool = Self {
            spawner: spawner::Spawner::new(shared),
        };

        (pool, Workers { workers })
    }

    pub fn spawn(&self, task: Task) {
        self.spawner.spawn(task)
    }
}

impl Workers {
    pub fn spawn(self) {
        self.workers.into_iter().for_each(|worker| {
            thread::Builder::new()
                .name(format!("hyyking-executor-{}", worker.id()))
                .spawn(move || worker.run())
                .expect("couldn't spawn thread");
        });
    }
}
