use super::Scheduled;
use slab::Slab;
use std::cell::RefCell;

pub struct Key(usize);

pub struct SuperSlab {
    inner: RefCell<Slab<Scheduled>>,
}

impl SuperSlab {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(Slab::with_capacity(256)),
        }
    }
    pub fn alloc(&self) -> Key {
        Key(self.inner.borrow_mut().insert(Scheduled::default()))
    }
    pub fn get(&self, k: Key) -> &Scheduled {
        self.inner.borrow().get(k.0)
    }
}

unsafe impl Sync for SuperSlab {}
unsafe impl Send for SuperSlab {}
