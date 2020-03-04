use super::core::{Cell, Core, Header, Trailer};
use super::state::Snapshot;
use super::{JoinError, Notified, Schedule, Task};

use std::future::Future;
use std::mem;
use std::panic;
use std::ptr::NonNull;
use std::task::{Poll, Waker};

pub(super) struct Harness<T: Future, S: 'static> {
    cell: NonNull<Cell<T, S>>,
}

impl<T, S> Harness<T, S>
where
    T: Future,
    S: 'static,
{
    pub(super) unsafe fn from_raw(ptr: NonNull<Header>) -> Harness<T, S> {
        Harness {
            cell: ptr.cast::<Cell<T, S>>(),
        }
    }

    fn header(&self) -> &Header {
        unsafe { &self.cell.as_ref().header }
    }

    fn trailer(&self) -> &Trailer {
        unsafe { &self.cell.as_ref().trailer }
    }

    fn core(&self) -> &Core<T, S> {
        unsafe { &self.cell.as_ref().core }
    }
}

impl<T, S> Harness<T, S>
where
    T: Future,
    S: Schedule,
{
    pub(super) fn poll(self) {
        let ref_inc = !self.core().is_bound();

        let snapshot = match self.header().state.transition_to_running(ref_inc) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                self.drop_reference();
                return;
            }
        };

        self.core().bind_scheduler(self.to_task());

        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            struct Guard<'a, T: Future, S: Schedule> {
                core: &'a Core<T, S>,
                polled: bool,
            }

            impl<T: Future, S: Schedule> Drop for Guard<'_, T, S> {
                fn drop(&mut self) {
                    if !self.polled {
                        self.core.drop_future_or_output();
                    }
                }
            }

            let mut guard = Guard {
                core: self.core(),
                polled: false,
            };

            if snapshot.is_cancelled() {
                Poll::Ready(Err(JoinError::cancelled()))
            } else {
                let res = guard.core.poll(self.header());

                guard.polled = true;
                res.map(Ok)
            }
        }));

        match res {
            Ok(Poll::Ready(out)) => {
                self.complete(out, snapshot.is_join_interested());
            }
            Ok(Poll::Pending) => match self.header().state.transition_to_idle() {
                Ok(snapshot) => {
                    if snapshot.is_notified() {
                        self.core().yield_now(Notified(self.to_task()));
                    }
                }
                Err(_) => self.cancel_task(),
            },
            Err(err) => {
                self.complete(Err(JoinError::panic(err)), snapshot.is_join_interested());
            }
        }
    }

    pub(super) fn dealloc(self) {
        self.trailer().waker.with_mut(|_| ());

        self.core().stage.with_mut(|_| {});
        self.core().scheduler.with_mut(|_| {});

        unsafe {
            drop(Box::from_raw(self.cell.as_ptr()));
        }
    }

    pub(super) fn try_read_output(self, dst: &mut Poll<super::Result<T::Output>>, waker: &Waker) {
        let snapshot = self.header().state.load();

        debug_assert!(snapshot.is_join_interested());

        if !snapshot.is_complete() {
            let res = if snapshot.has_join_waker() {
                let will_wake = unsafe {
                    self.trailer()
                        .waker
                        .with(|ptr| (*ptr).as_ref().unwrap().will_wake(waker))
                };

                if will_wake {
                    return;
                }

                self.header()
                    .state
                    .unset_waker()
                    .and_then(|snapshot| self.set_join_waker(waker.clone(), snapshot))
            } else {
                self.set_join_waker(waker.clone(), snapshot)
            };

            match res {
                Ok(_) => return,
                Err(snapshot) => {
                    assert!(snapshot.is_complete());
                }
            }
        }

        *dst = Poll::Ready(self.core().take_output());
    }

    fn set_join_waker(&self, waker: Waker, snapshot: Snapshot) -> Result<Snapshot, Snapshot> {
        assert!(snapshot.is_join_interested());
        assert!(!snapshot.has_join_waker());

        unsafe {
            self.trailer().waker.with_mut(|ptr| {
                *ptr = Some(waker);
            });
        }

        let res = self.header().state.set_join_waker();

        if res.is_err() {
            unsafe {
                self.trailer().waker.with_mut(|ptr| {
                    *ptr = None;
                });
            }
        }

        res
    }

    pub(super) fn drop_join_handle_slow(self) {
        if self.header().state.unset_join_interested().is_err() {
            self.core().drop_future_or_output();
        }
        self.drop_reference();
    }

    pub(super) fn wake_by_val(self) {
        self.wake_by_ref();
        self.drop_reference();
    }

    pub(super) fn wake_by_ref(&self) {
        if self.header().state.transition_to_notified() {
            self.core().schedule(Notified(self.to_task()));
        }
    }

    pub(super) fn drop_reference(self) {
        if self.header().state.ref_dec() {
            self.dealloc();
        }
    }

    pub(super) fn shutdown(self) {
        if !self.header().state.transition_to_shutdown() {
            return;
        }
        self.cancel_task();
    }

    fn cancel_task(self) {
        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            self.core().drop_future_or_output();
        }));

        if let Err(err) = res {
            self.complete(Err(JoinError::panic(err)), true);
        } else {
            self.complete(Err(JoinError::cancelled()), true);
        }
    }

    fn complete(mut self, output: super::Result<T::Output>, is_join_interested: bool) {
        if is_join_interested {
            self.core().store_output(output);
            self.transition_to_complete();
        }

        let ref_dec = if self.core().is_bound() {
            if let Some(task) = self.core().release(self.to_task()) {
                mem::forget(task);
                true
            } else {
                false
            }
        } else {
            false
        };

        let snapshot = self
            .header()
            .state
            .transition_to_terminal(!is_join_interested, ref_dec);

        if snapshot.ref_count() == 0 {
            self.dealloc()
        }
    }

    fn transition_to_complete(&mut self) {
        let snapshot = self.header().state.transition_to_complete();

        if !snapshot.is_join_interested() {
            self.core().drop_future_or_output();
        } else if snapshot.has_join_waker() {
            self.wake_join();
        }
    }

    fn wake_join(&self) {
        self.trailer().waker.with(|ptr| match unsafe { &*ptr } {
            Some(waker) => waker.wake_by_ref(),
            None => panic!("waker missing"),
        });
    }

    fn to_task(&self) -> Task<S> {
        unsafe { Task::from_raw(self.header().into()) }
    }
}
