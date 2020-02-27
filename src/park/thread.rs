use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::Duration;

use crate::io;
use crate::park::{Park, Unpark};

#[derive(Debug)]
struct ParkerState(AtomicUsize);

impl ParkerState {
    const EMPTY: usize = 0b0000;
    const NOTIFIED: usize = 0b001;
    const PARKED_COND: usize = 0b0010;
    const PARKED_DRIV: usize = 0b0100;

    fn new() -> Self {
        Self(AtomicUsize::new(Self::EMPTY))
    }

    fn is_notified(&self) -> bool {
        self.compare_exchange(
            Self::NOTIFIED,
            Self::EMPTY,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .is_ok()
    }

    fn consume_notification(&self) -> Option<()> {
        self.compare_exchange(
            Self::NOTIFIED,
            Self::EMPTY,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .ok()
        .map(|_| {})
    }

    fn update_from_empty(&self, s: usize) -> Option<()> {
        self.compare_exchange(Self::EMPTY, s, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| {})
            .or_else(|value| {
                if value == Self::NOTIFIED {
                    let old = self.swap(Self::EMPTY, Ordering::SeqCst);
                    debug_assert_eq!(old, Self::NOTIFIED, "park state changed unexpectedly");
                    return Err(());
                }
                panic!("inconsistent park_timeout state; actual = {}", value);
            })
            .ok()
    }
}

impl std::ops::Deref for ParkerState {
    type Target = AtomicUsize;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct Parker(Arc<Inner>);
pub struct UnParker(Arc<Inner>);

struct Inner {
    state: ParkerState,
    m: Mutex<()>,
    c: Condvar,
    shared: Arc<Shared>,
}

struct Shared {
    handle: <io::Driver as Park>::Handle,
    driver: Mutex<io::Driver>,
}

impl Parker {
    pub fn new(driver: io::Driver) -> Self {
        let handle = driver.handle();
        Self(Arc::new(Inner {
            state: ParkerState::new(),
            m: Mutex::new(()),
            c: Condvar::new(),
            shared: Arc::new(Shared {
                handle,
                driver: Mutex::new(driver),
            }),
        }))
    }
}

impl Clone for Parker {
    fn clone(&self) -> Self {
        Self(Arc::new(Inner {
            state: ParkerState::new(),
            m: Mutex::new(()),
            c: Condvar::new(),
            shared: self.0.shared.clone(),
        }))
    }
}

impl Park for Parker {
    type Handle = UnParker;

    fn handle(&self) -> Self::Handle {
        UnParker(self.0.clone())
    }
    fn park(&mut self) -> Result<(), std::io::Error> {
        self.0.park();
        Ok(())
    }
    fn park_timeout(&mut self, dur: Duration) -> Result<(), std::io::Error> {
        self.0.park_timeout(dur);
        Ok(())
    }
}

impl Unpark for UnParker {
    fn unpark(&self) {
        self.0.unpark()
    }
}

impl Inner {
    fn park(&self) -> Option<()> {
        for _ in 0..3 {
            self.state.consume_notification()?;
            std::thread::yield_now();
        }

        if let Some(ref mut driver) = self.shared.driver.try_lock().ok() {
            self.state.update_from_empty(ParkerState::PARKED_DRIV)?;

            driver.park().expect("couldn't park driver");

            match self.state.swap(ParkerState::EMPTY, Ordering::SeqCst) {
                ParkerState::NOTIFIED | ParkerState::PARKED_DRIV => Some(()),
                n => panic!("inconsistent park_timeout state: {}", n),
            }
        } else {
            let lock = self.m.lock().unwrap();

            self.state.update_from_empty(ParkerState::PARKED_COND)?;
            let _ = self.c.wait_while(lock, |_| self.state.is_notified());
            Some(())
        }
    }

    fn park_timeout(&self, dur: Duration) -> Option<()> {
        if let Some(ref mut driver) = self.shared.driver.try_lock().ok() {
            driver.park_timeout(dur).ok()
        } else {
            let lock = self.m.lock().unwrap();
            let _ = self.c.wait_timeout(lock, dur);

            match self.state.swap(ParkerState::EMPTY, Ordering::SeqCst) {
                ParkerState::NOTIFIED | ParkerState::PARKED_COND | ParkerState::PARKED_DRIV => {
                    Some(())
                }
                n => panic!("inconsistent park_timeout state: {}", n),
            }
        }
    }

    fn unpark(&self) {
        match self.state.swap(ParkerState::NOTIFIED, Ordering::SeqCst) {
            ParkerState::EMPTY | ParkerState::NOTIFIED => {}

            ParkerState::PARKED_COND => {
                drop(self.m.lock().unwrap());
                self.c.notify_one()
            }

            ParkerState::PARKED_DRIV => {
                self.shared.handle.unpark();
            }
            actual => panic!("inconsistent state in unpark; actual = {}", actual),
        }
    }
}
