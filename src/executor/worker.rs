use crossbeam::deque::{Injector, Stealer, Worker as LocalQueue};
use std::sync::Arc;

use super::Task;

pub struct Shared {
    global: Injector<Task>,
    stealers: Box<[Stealer<Task>]>,
}

pub struct Worker {
    idx: usize,
    local: LocalQueue<Task>,
    shared: Arc<Shared>,
}

pub fn create_set(n: usize) -> (Arc<Shared>, Vec<Worker>) {
    let injector = Injector::new();
    let local: Vec<_> = (0..n).map(|_| LocalQueue::new_fifo()).collect();
    let stealers: Vec<_> = local.iter().map(|lq| lq.stealer()).collect();

    let shared = Arc::new(Shared {
        global: injector,
        stealers: stealers.into_boxed_slice(),
    });

    let workers = local
        .into_iter()
        .enumerate()
        .map(|(idx, local)| Worker::new(idx, local, shared.clone()))
        .collect();
    (shared, workers)
}

impl Worker {
    fn new(idx: usize, local: LocalQueue<Task>, shared: Arc<Shared>) -> Self {
        Self { idx, local, shared }
    }
    pub fn id(&self) -> usize {
        self.idx
    }
    pub fn run(self) {
        let me = self;

        loop {
            if let Some(task) = find_task(&me.local, &me.shared.global, &me.shared.stealers) {
                task.run(&me.local);
            }
        }
    }
}

impl Shared {
    pub fn schedule(&self, task: Task) {
        self.global.push(task)
    }
}

fn find_task<T>(local: &LocalQueue<T>, global: &Injector<T>, stealers: &[Stealer<T>]) -> Option<T> {
    // Pop a task from the local queue, if not empty.
    local.pop().or_else(|| {
        // Otherwise, we need to look for a task elsewhere.
        std::iter::repeat_with(|| {
            // Try stealing a batch of tasks from the global queue.
            global
                .steal_batch_and_pop(local)
                // Or try stealing a task from one of the other threads.
                .or_else(|| stealers.iter().map(|s| s.steal()).collect())
        })
        // Loop while no task was stolen and any steal operation needs to be retried.
        .find(|s| !s.is_retry())
        // Extract the stolen task, if there is one.
        .and_then(|s| s.success())
    })
}
