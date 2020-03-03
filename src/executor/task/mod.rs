use std::future::Future;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::task::Context;

use crossbeam::deque::Worker as LocalQueue;

pub type JoinHandle<R> = Pin<Box<dyn Future<Output = R> + Send>>;

pub struct Task;

impl Task {
    pub fn run(self, _local: &LocalQueue<Self>) {}
}

pub fn joinable<F>(f: F) -> (Task, JoinHandle<F::Output>)
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let (s, r) = futures::channel::oneshot::channel();

    let _future = async move {
        let _ = s.send(f.await);
    };

    let t = Task {};

    let j = Box::pin(async { r.await.expect("couldn't receive task") });

    (t, j)
}
