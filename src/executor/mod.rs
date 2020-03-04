mod atomic_cell;
mod causal_cell;
mod linked_list;
mod park;
mod task;
mod worker;

use std::future::Future;
use std::sync::Arc;

use self::task::JoinHandle;

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

pub struct ThreadPool {
    spawner: Spawner,
}

#[derive(Clone)]
pub struct Spawner {
    shared: Arc<worker::Shared>,
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

impl ThreadPool {
    pub fn new() -> (Self, worker::Launch) {
        const N: usize = 4;

        let (shared, launch) = worker::create_set(N);

        let pool = Self {
            spawner: Spawner { shared },
        };

        (pool, launch)
    }

    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.spawner.spawn(future)
    }

    pub(crate) fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        // TODO: Recreate a block_on function;
        todo!()
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.spawner.shared.close();
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

impl Spawner {
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        todo!()
    }
}
