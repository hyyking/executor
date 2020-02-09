use std::{
    collections::HashMap,
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, RwLock,
    },
};

use crate::registration::Registration;

use futures::task::AtomicWaker;

pub type Handle = Arc<RwLock<HashMap<mio::Token, Scheduled>>>;

pub struct Driver {
    io: mio::Poll,
    events: mio::Events,
    // net: mio::SetReadiness,
    map: Handle,
}

pub struct Scheduled {
    pub readiness: AtomicUsize,
    pub reader: AtomicWaker,
    pub writer: AtomicWaker,
}

impl Driver {
    pub fn new() -> Self {
        Self {
            io: mio::Poll::new().unwrap(),
            events: mio::Events::with_capacity(1024),
            map: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    pub fn handle(&self) -> Handle {
        self.map.clone()
    }
    pub fn register(&self, token: mio::Token, source: &dyn mio::Evented) -> Registration {
        Registration::new(self, token, source)
    }
    pub fn add_io(&self, token: mio::Token, source: &dyn mio::Evented) {
        self.map.write().unwrap().insert(
            token.clone(),
            Scheduled {
                readiness: AtomicUsize::new(mio::Ready::empty().as_usize()),
                reader: AtomicWaker::new(),
                writer: AtomicWaker::new(),
            },
        );
        let _ = self
            .io
            .register(source, token, mio::Ready::all(), mio::PollOpt::edge());
    }

    pub fn turn(&mut self) -> io::Result<()> {
        let io = &mut self.io;

        io.poll(&mut self.events, None)?;

        for event in self.events.iter() {
            println!("{:?}", event);
            self.dispatch(event);
        }
        Ok(())
    }
    pub fn dispatch(&self, e: mio::Event) {
        let token = e.token();
        let kind = e.readiness();

        let mut rl = self.map.write().expect("couldn't access reader");

        let io = match rl.get_mut(&token) {
            Some(io) => io,
            None => return,
        };

        io.set_readiness(|old| old | kind.as_usize());

        if kind.is_writable() {
            io.writer.wake();
        }

        if !(kind & (!mio::Ready::writable())).is_empty() {
            io.reader.wake();
        }
    }
}

impl Scheduled {
    fn set_readiness<F: FnOnce(usize) -> usize>(&mut self, f: F) {
        let current = self.readiness.load(Ordering::Acquire);
        self.readiness.store(f(current), Ordering::Relaxed);
    }
}
