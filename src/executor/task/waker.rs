use std::future::Future;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ops;
use std::ptr::NonNull;
use std::task::{RawWaker, RawWakerVTable, Waker};

use super::harness::Harness;
use super::{Header, Schedule};

pub struct WakerRef<'a, S: 'static> {
    waker: ManuallyDrop<Waker>,
    _p: PhantomData<(&'a Header, S)>,
}

pub(super) fn waker_ref<T, S>(header: &Header) -> WakerRef<'_, S>
where
    T: Future,
    S: Schedule,
{
    let waker = unsafe { ManuallyDrop::new(Waker::from_raw(raw_waker::<T, S>(header))) };

    WakerRef {
        waker,
        _p: PhantomData,
    }
}

impl<S> ops::Deref for WakerRef<'_, S> {
    type Target = Waker;

    fn deref(&self) -> &Waker {
        &self.waker
    }
}

fn raw_waker<T, S>(header: *const Header) -> RawWaker
where
    T: Future,
    S: Schedule,
{
    let ptr = header as *const ();
    let vtable = &RawWakerVTable::new(
        clone_waker::<T, S>,
        wake_by_val::<T, S>,
        wake_by_ref::<T, S>,
        drop_waker::<T, S>,
    );
    RawWaker::new(ptr, vtable)
}

unsafe fn clone_waker<T, S>(ptr: *const ()) -> RawWaker
where
    T: Future,
    S: Schedule,
{
    let header = ptr as *const Header;
    (*header).state.ref_inc();
    raw_waker::<T, S>(header)
}

unsafe fn drop_waker<T, S>(ptr: *const ())
where
    T: Future,
    S: Schedule,
{
    let ptr = NonNull::new_unchecked(ptr as *mut Header);
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.drop_reference();
}

unsafe fn wake_by_val<T, S>(ptr: *const ())
where
    T: Future,
    S: Schedule,
{
    let ptr = NonNull::new_unchecked(ptr as *mut Header);
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.wake_by_val();
}

unsafe fn wake_by_ref<T, S>(ptr: *const ())
where
    T: Future,
    S: Schedule,
{
    let ptr = NonNull::new_unchecked(ptr as *mut Header);
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.wake_by_ref();
}