use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ptr::NonNull;

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

pub unsafe trait Link {
    type Handle;
    type Target;

    fn as_raw(handle: &Self::Handle) -> NonNull<Self::Target>;

    unsafe fn from_raw(ptr: NonNull<Self::Target>) -> Self::Handle;
    unsafe fn pointers(target: NonNull<Self::Target>) -> NonNull<Pointers<Self::Target>>;
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug)]
pub struct LinkedList<T: Link> {
    head: Option<NonNull<T::Target>>,
    tail: Option<NonNull<T::Target>>,
}

#[derive(Debug)]
pub struct Pointers<T> {
    prev: Option<NonNull<T>>,
    next: Option<NonNull<T>>,
}

pub struct Iter<'a, T: Link> {
    curr: Option<NonNull<T::Target>>,
    _p: PhantomData<&'a T>,
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

unsafe impl<T: Link> Send for LinkedList<T> where T::Target: Send {}
unsafe impl<T: Link> Sync for LinkedList<T> where T::Target: Sync {}

unsafe impl<T: Send> Send for Pointers<T> {}
unsafe impl<T: Sync> Sync for Pointers<T> {}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

impl<T: Link> LinkedList<T> {
    pub fn new() -> LinkedList<T> {
        LinkedList {
            head: None,
            tail: None,
        }
    }

    pub fn push_front(&mut self, val: T::Handle) {
        let val = ManuallyDrop::new(val);
        let ptr = T::as_raw(&*val);

        unsafe {
            T::pointers(ptr).as_mut().next = self.head;
            T::pointers(ptr).as_mut().prev = None;

            if let Some(head) = self.head {
                T::pointers(head).as_mut().prev = Some(ptr);
            }

            self.head = Some(ptr);

            if self.tail.is_none() {
                self.tail = Some(ptr);
            }
        }
    }

    pub fn pop_back(&mut self) -> Option<T::Handle> {
        unsafe {
            let last = self.tail?;
            self.tail = T::pointers(last).as_ref().prev;

            if let Some(prev) = T::pointers(last).as_ref().prev {
                T::pointers(prev).as_mut().next = None;
            } else {
                self.head = None
            }

            T::pointers(last).as_mut().prev = None;
            T::pointers(last).as_mut().next = None;

            Some(T::from_raw(last))
        }
    }

    pub fn is_empty(&self) -> bool {
        if self.head.is_some() {
            return false;
        }

        assert!(self.tail.is_none());
        true
    }

    pub unsafe fn remove(&mut self, node: NonNull<T::Target>) -> Option<T::Handle> {
        if let Some(prev) = T::pointers(node).as_ref().prev {
            debug_assert_eq!(T::pointers(prev).as_ref().next, Some(node));
            T::pointers(prev).as_mut().next = T::pointers(node).as_ref().next;
        } else {
            if self.head != Some(node) {
                return None;
            }

            self.head = T::pointers(node).as_ref().next;
        }

        if let Some(next) = T::pointers(node).as_ref().next {
            debug_assert_eq!(T::pointers(next).as_ref().prev, Some(node));
            T::pointers(next).as_mut().prev = T::pointers(node).as_ref().prev;
        } else {
            if self.tail != Some(node) {
                return None;
            }

            self.tail = T::pointers(node).as_ref().prev;
        }

        T::pointers(node).as_mut().next = None;
        T::pointers(node).as_mut().prev = None;

        Some(T::from_raw(node))
    }
}

impl<T: Link> LinkedList<T> {
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            curr: self.head,
            _p: PhantomData,
        }
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

impl<'a, T: Link> Iterator for Iter<'a, T> {
    type Item = &'a T::Target;

    fn next(&mut self) -> Option<&'a T::Target> {
        let curr = self.curr?;
        self.curr = unsafe { T::pointers(curr).as_ref() }.next;

        Some(unsafe { &*curr.as_ptr() })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~

impl<T> Pointers<T> {
    pub(crate) fn new() -> Pointers<T> {
        Pointers {
            prev: None,
            next: None,
        }
    }
}
