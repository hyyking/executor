use std::cell::UnsafeCell;
use std::future::Future;
use std::pin::Pin;
use std::ptr::NonNull;
use std::task::{Context, Poll, Waker};

use super::raw::{self, Vtable};
use super::state::State;
use super::waker::waker_ref;
use super::{Notified, Schedule, Task};
use crate::executor::{causal_cell::CausalCell, linked_list};

#[repr(C)]
pub struct Cell<T: Future, S> {
    pub(super) header: Header,

    pub(super) core: Core<T, S>,

    pub(super) trailer: Trailer,
}

pub struct Core<T: Future, S> {
    pub(super) scheduler: CausalCell<Option<S>>,

    pub(super) stage: CausalCell<Stage<T>>,
}

#[repr(C)]
pub struct Header {
    pub(super) state: State,

    pub(crate) owned: UnsafeCell<linked_list::Pointers<Header>>,

    pub(crate) queue_next: UnsafeCell<Option<NonNull<Header>>>,

    pub(super) stack_next: UnsafeCell<Option<NonNull<Header>>>,

    pub(super) vtable: &'static Vtable,
}

unsafe impl Send for Header {}
unsafe impl Sync for Header {}

pub(super) struct Trailer {
    pub(super) waker: CausalCell<Option<Waker>>,
}

pub(super) enum Stage<T: Future> {
    Running(T),
    Finished(super::Result<T::Output>),
    Consumed,
}

impl<T: Future, S: Schedule> Cell<T, S> {
    pub(super) fn new(future: T, state: State) -> Box<Cell<T, S>> {
        Box::new(Cell {
            header: Header {
                state,
                owned: UnsafeCell::new(linked_list::Pointers::new()),
                queue_next: UnsafeCell::new(None),
                stack_next: UnsafeCell::new(None),
                vtable: raw::vtable::<T, S>(),
            },
            core: Core {
                scheduler: CausalCell::new(None),
                stage: CausalCell::new(Stage::Running(future)),
            },
            trailer: Trailer {
                waker: CausalCell::new(None),
            },
        })
    }
}

impl<T: Future, S: Schedule> Core<T, S> {
    pub(super) fn bind_scheduler(&self, task: Task<S>) {
        use std::mem::ManuallyDrop;

        let task = ManuallyDrop::new(task);

        if self.is_bound() {
            return;
        }

        let scheduler = S::bind(ManuallyDrop::into_inner(task));

        self.scheduler.with_mut(|ptr| unsafe {
            *ptr = Some(scheduler);
        });
    }

    pub(super) fn is_bound(&self) -> bool {
        self.scheduler.with(|ptr| unsafe { (*ptr).is_some() })
    }

    pub(super) fn poll(&self, header: &Header) -> Poll<T::Output> {
        let res = {
            self.stage.with_mut(|ptr| {
                let future = match unsafe { &mut *ptr } {
                    Stage::Running(future) => future,
                    _ => unreachable!("unexpected stage"),
                };

                let future = unsafe { Pin::new_unchecked(future) };

                let waker_ref = waker_ref::<T, S>(header);
                let mut cx = Context::from_waker(&*waker_ref);

                future.poll(&mut cx)
            })
        };

        if res.is_ready() {
            self.drop_future_or_output();
        }

        res
    }

    pub(super) fn drop_future_or_output(&self) {
        self.stage.with_mut(|ptr| {
            unsafe { *ptr = Stage::Consumed };
        });
    }

    pub(super) fn store_output(&self, output: super::Result<T::Output>) {
        self.stage.with_mut(|ptr| {
            unsafe { *ptr = Stage::Finished(output) };
        });
    }

    pub(super) fn take_output(&self) -> super::Result<T::Output> {
        use std::mem;

        self.stage.with_mut(
            |ptr| match mem::replace(unsafe { &mut *ptr }, Stage::Consumed) {
                Stage::Finished(output) => output,
                _ => panic!("unexpected task state"),
            },
        )
    }

    pub(super) fn schedule(&self, task: Notified<S>) {
        self.scheduler.with(|ptr| match unsafe { &*ptr } {
            Some(scheduler) => scheduler.schedule(task),
            None => panic!("no scheduler set"),
        });
    }

    pub(super) fn yield_now(&self, task: Notified<S>) {
        self.scheduler.with(|ptr| match unsafe { &*ptr } {
            Some(scheduler) => scheduler.yield_now(task),
            None => panic!("no scheduler set"),
        });
    }

    pub(super) fn release(&self, task: Task<S>) -> Option<Task<S>> {
        use std::mem::ManuallyDrop;

        let task = ManuallyDrop::new(task);

        self.scheduler.with(|ptr| match unsafe { &*ptr } {
            Some(scheduler) => scheduler.release(&*task),
            None => None,
        })
    }
}

impl Header {
    pub(crate) fn shutdown(&self) {
        use super::RawTask;

        let task = unsafe { RawTask::from_raw(self.into()) };
        task.shutdown();
    }
}
