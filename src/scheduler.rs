use std::future::Future;
use std::mem::{self, ManuallyDrop};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use crate::park::{Park, Unpark};

#[allow(dead_code)]
const VTABLE: RawWakerVTable = RawWakerVTable::new(
    |clone: *const ()| unsafe {
        let arc = ManuallyDrop::new(Arc::from_raw(clone as *const UnparkState));
        mem::forget(arc.clone());
        RawWaker::new(clone, &VTABLE)
    },
    |wake: *const ()| unsafe {
        let up = &Arc::from_raw(wake as *const UnparkState);
        up.unpark.unpark();
    },
    |wake_by_ref: *const ()| unsafe {
        let up = ManuallyDrop::new(Arc::from_raw(wake_by_ref as *const UnparkState));
        up.unpark.unpark();
    },
    |drop_waker: *const ()| unsafe {
        drop(Arc::from_raw(drop_waker as *const UnparkState));
    },
);

pub struct Executor<P> {
    park: P,
    state: Arc<UnparkState>,
}

struct UnparkState {
    unpark: Box<dyn Unpark>,
}

impl<P: Park> Executor<P> {
    pub fn new(park: P) -> Self {
        let unpark = park.handle();
        Self {
            park,
            state: Arc::new(UnparkState {
                unpark: Box::new(unpark),
            }),
        }
    }

    pub fn block_on<F: Future>(&mut self, mut f: F) -> F::Output {
        let mut f = unsafe { Pin::new_unchecked(&mut f) };

        let raw_waker = RawWaker::new(&*self.state as *const UnparkState as *const (), &VTABLE);
        let waker = ManuallyDrop::new(unsafe { Waker::from_raw(raw_waker) });

        let mut cx = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(o) = f.as_mut().poll(&mut cx) {
                return o;
            }
            // tick the scheduler

            self.park
                .park_timeout(std::time::Duration::from_millis(0))
                .ok()
                .expect("problem parking");
        }
    }
}
