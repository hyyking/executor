use std::sync::Arc;

use super::task::Task;
use super::worker::Shared;

pub struct Spawner {
    inner: Arc<Shared>,
}

impl Spawner {
    pub fn new(inner: Arc<Shared>) -> Self {
        Self { inner }
    }

    pub fn spawn(&self, task: Task) {
        self.inner.schedule(task)
    }
}
