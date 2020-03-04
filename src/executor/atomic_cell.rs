use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering::AcqRel};

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

pub struct AtomicCell<T> {
    ptr: AtomicPtr<T>,
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

unsafe impl<T: Send> Send for AtomicCell<T> {}
unsafe impl<T: Send> Sync for AtomicCell<T> {}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

impl<T> AtomicCell<T> {
    pub(super) fn new(data: Option<Box<T>>) -> Self {
        let ptr = data.map(Box::into_raw).unwrap_or(ptr::null_mut());
        Self {
            ptr: AtomicPtr::new(ptr),
        }
    }

    pub(super) fn swap(&self, val: Option<Box<T>>) -> Option<Box<T>> {
        let raw = val.map(Box::into_raw).unwrap_or(ptr::null_mut());

        let old = self.ptr.swap(raw, AcqRel);

        if old.is_null() {
            None
        } else {
            Some(unsafe { Box::from_raw(old) })
        }
    }

    pub(super) fn set(&self, val: Box<T>) {
        let _ = self.swap(Some(val));
    }

    pub(super) fn take(&self) -> Option<Box<T>> {
        self.swap(None)
    }
}

impl<T> Drop for AtomicCell<T> {
    fn drop(&mut self) {
        // Free any data still held by the cell
        let _ = self.take();
    }
}
