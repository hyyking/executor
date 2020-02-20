use std::future::Future;
use std::mem::{self, ManuallyDrop};
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

#[derive(Default)]
struct Parker(Mutex<bool>, Condvar);

fn unpark(p: &Parker) {
    *p.0.lock().unwrap() = true;
    p.1.notify_one();
}

#[allow(dead_code)]
const VTABLE: RawWakerVTable = RawWakerVTable::new(
    |clone: *const ()| unsafe {
        let arc = ManuallyDrop::new(Arc::from_raw(clone as *const Parker));
        mem::forget(arc.clone());
        RawWaker::new(clone, &VTABLE)
    },
    |wake: *const ()| unsafe {
        unpark(&Arc::from_raw(wake as *const Parker));
    },
    |wake_by_ref: *const ()| unsafe {
        unpark(&ManuallyDrop::new(Arc::from_raw(
            wake_by_ref as *const Parker,
        )));
    },
    |drop_waker: *const ()| unsafe {
        drop(Arc::from_raw(drop_waker as *const Parker));
    },
);

pub fn block_on<F: Future>(mut f: F) -> F::Output {
    let mut f = unsafe { Pin::new_unchecked(&mut f) };

    std::thread_local! {
        static CACHE: std::cell::RefCell<(Arc<Parker>, Waker)> = {
            let park = Arc::new(Parker::default());
            let notifier = Arc::into_raw(park.clone());
            let waker = unsafe { Waker::from_raw(RawWaker::new(notifier as *const _, &VTABLE)) };
            std::cell::RefCell::new((park, waker))
        };
    }
    CACHE.with(|cell| {
        let (ref park, ref waker) = *cell.borrow();

        let mut cx = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(o) = f.as_mut().poll(&mut cx) {
                return o;
            }
            assert!(park
                .1
                .wait_while(park.0.lock().unwrap(), |started| {
                    if *started {
                        *started = false;
                        return true;
                    }
                    false
                })
                .is_ok())
        }
    })
}
