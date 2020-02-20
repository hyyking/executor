use std::sync::atomic::{AtomicUsize, Ordering};

use futures::task::AtomicWaker;

pub struct Scheduled {
    pub readiness: AtomicUsize,
    pub reader: AtomicWaker,
    pub writer: AtomicWaker,
}

impl Scheduled {
    pub fn get_readiness(&self) -> usize {
        self.readiness.load(Ordering::Acquire)
    }
    pub fn set_readiness(&self, f: impl Fn(usize) -> usize) -> usize {
        let mut current = self.readiness.load(Ordering::Acquire);
        loop {
            let current_readiness = current & mio::Ready::all().as_usize();
            let new = f(current_readiness);

            match self
                .readiness
                .compare_exchange(current, new, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return current,
                // we lost the race, retry!
                Err(actual) => current = actual,
            }
        }
    }
}
