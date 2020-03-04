use std::cell::RefCell;
use std::sync::{atomic::AtomicPtr, Arc, Mutex};

use super::atomic_cell::AtomicCell;
use super::linked_list::LinkedList;
use super::park::{Parker, Unparker};
use super::task;

use crossbeam::deque::{Injector, Stealer, Worker as LocalQueue};

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

type Task = task::Task<Arc<Worker>>;

crate::scoped_thread_local!(static CURRENT: Context);

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

pub struct Worker {
    idx: usize,
    shared: Arc<Shared>,
    core: AtomicCell<Core>,
}

pub struct Core {
    tick: u8,

    // local queue
    local: LocalQueue<Arc<Worker>>,

    // state
    is_shutdown: bool,
    is_searching: bool,

    // tasks
    tasks: LinkedList<Task>,

    // parker
    parker: Option<Parker>,

    rand: (),
}

pub struct Shared {
    remote: Box<[Remote]>,
    global: Injector<Arc<Worker>>,

    idle: (),

    shutdown_workers: Mutex<Vec<(Box<Core>, Arc<Worker>)>>,
}

pub struct Remote {
    stealer: Stealer<Arc<Worker>>,

    pending_drop: (),

    unpark: Unparker,
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

pub struct Context {
    worker: Arc<Worker>,
    core: RefCell<Option<Box<Core>>>,
}

pub struct Launch(Vec<Arc<Worker>>);

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

pub fn create_set(n: usize) -> (Arc<Shared>, Launch) {
    todo!()
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

impl Worker {
    fn run(&self) {
        todo!()
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

impl Shared {
    pub fn schedule(&self, task: Task, _: bool) {
        todo!()
    }
    pub fn close(&self) {}
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

impl Launch {
    pub fn launch(mut self) {
        use std::thread;
        self.0.drain(..).for_each(|worker| {
            thread::Builder::new()
                .name(format!("hyyking-executor-{}", worker.idx))
                .spawn(move || worker.run())
                .expect("couldn't spawn thread");
        });
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
