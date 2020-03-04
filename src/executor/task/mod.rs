mod core;
mod error;
mod harness;
mod join;
mod raw;
mod stack;
mod state;
mod waker;

pub use self::core::Header;
pub use self::error::JoinError;
pub use self::join::JoinHandle;
pub use self::stack::TransferStack;

use self::core::Cell;
use self::harness::Harness;
use self::raw::RawTask;
use self::state::State;

use crate::executor::linked_list;

use std::future::Future;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::{fmt, mem};

#[repr(transparent)]
pub(crate) struct Task<S: 'static> {
    raw: RawTask,
    _p: PhantomData<S>,
}

unsafe impl<S> Send for Task<S> {}
unsafe impl<S> Sync for Task<S> {}

#[repr(transparent)]
pub(crate) struct Notified<S: 'static>(Task<S>);

unsafe impl<S: Schedule> Send for Notified<S> {}
unsafe impl<S: Schedule> Sync for Notified<S> {}

pub(crate) type Result<T> = std::result::Result<T, JoinError>;

pub(crate) trait Schedule: Sync + Sized + 'static {
    fn bind(task: Task<Self>) -> Self;

    fn release(&self, task: &Task<Self>) -> Option<Task<Self>>;

    fn schedule(&self, task: Notified<Self>);

    fn yield_now(&self, task: Notified<Self>) {
        self.schedule(task);
    }
}

pub(crate) fn joinable<T, S>(task: T) -> (Notified<S>, JoinHandle<T::Output>)
where
    T: Future + Send + 'static,
    S: Schedule,
{
    let raw = RawTask::new::<_, S>(task);

    let task = Task {
        raw,
        _p: PhantomData,
    };

    let join = JoinHandle::new(raw);

    (Notified(task), join)
}

impl<S: 'static> Task<S> {
    pub(crate) unsafe fn from_raw(ptr: NonNull<Header>) -> Task<S> {
        Task {
            raw: RawTask::from_raw(ptr),
            _p: PhantomData,
        }
    }

    pub(crate) fn header(&self) -> &Header {
        self.raw.header()
    }
}

impl<S: 'static> Notified<S> {
    pub(crate) unsafe fn from_raw(ptr: NonNull<Header>) -> Notified<S> {
        Notified(Task::from_raw(ptr))
    }

    pub(crate) fn header(&self) -> &Header {
        self.0.header()
    }
}

impl<S: 'static> Task<S> {
    pub(crate) fn into_raw(self) -> NonNull<Header> {
        let ret = self.header().into();
        mem::forget(self);
        ret
    }
}

impl<S: 'static> Notified<S> {
    pub(crate) fn into_raw(self) -> NonNull<Header> {
        self.0.into_raw()
    }
}

impl<S: Schedule> Task<S> {
    /// Pre-emptively cancel the task as part of the shutdown process.
    pub(crate) fn shutdown(&self) {
        self.raw.shutdown();
    }
}

impl<S: Schedule> Notified<S> {
    /// Run the task
    pub(crate) fn run(self) {
        self.0.raw.poll();
        mem::forget(self);
    }

    /// Pre-emptively cancel the task as part of the shutdown process.
    pub(crate) fn shutdown(self) {
        self.0.shutdown();
    }
}

impl<S: 'static> Drop for Task<S> {
    fn drop(&mut self) {
        // Decrement the ref count
        if self.header().state.ref_dec() {
            // Deallocate if this is the final ref count
            self.raw.dealloc();
        }
    }
}

impl<S> fmt::Debug for Task<S> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "Task({:p})", self.header())
    }
}

impl<S> fmt::Debug for Notified<S> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "task::Notified({:p})", self.0.header())
    }
}

unsafe impl<S> linked_list::Link for Task<S> {
    type Handle = Task<S>;
    type Target = Header;

    fn as_raw(handle: &Task<S>) -> NonNull<Header> {
        handle.header().into()
    }

    unsafe fn from_raw(ptr: NonNull<Header>) -> Task<S> {
        Task::from_raw(ptr)
    }

    unsafe fn pointers(target: NonNull<Header>) -> NonNull<linked_list::Pointers<Header>> {
        NonNull::from(&mut *target.as_ref().owned.get())
    }
}
