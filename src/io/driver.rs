use std::{
    collections::HashMap,
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        RwLock, {Arc, Weak},
    },
    task::Waker,
};

use super::{Registration, Scheduled, SuperSlab};

use futures::task::AtomicWaker;

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum Direction {
    Read,
    Write,
}

pub struct Driver {
    events: mio::Events,
    inner: Arc<Inner>,
}

#[derive(Clone)]
pub struct Handle {
    inner: Weak<Inner>,
}

pub struct Inner {
    io: mio::Poll,
    map: RwLock<HashMap<mio::Token, Scheduled>>,
    n_sources: AtomicUsize,
}

impl Driver {
    pub fn new() -> Self {
        Self {
            events: mio::Events::with_capacity(1024),
            inner: Arc::new(Inner {
                io: mio::Poll::new().unwrap(),
                map: RwLock::new(HashMap::new()),
                n_sources: AtomicUsize::new(0),
            }),
        }
    }
    pub fn empty(&self) -> bool {
        self.inner.n_sources.load(Ordering::SeqCst) == 0
    }
    pub fn handle(&self) -> Handle {
        Handle {
            inner: Arc::downgrade(&self.inner),
        }
    }
    #[allow(dead_code)]
    pub fn register(
        &self,
        token: mio::Token,
        source: &dyn mio::Evented,
    ) -> io::Result<Registration> {
        Registration::new(self.handle(), token, source)
    }
    pub fn turn(&mut self) -> io::Result<()> {
        let Inner {
            ref io, ref map, ..
        } = *self.inner;

        match io.poll(&mut self.events, None) {
            Ok(_) => {}
            Err(e) => return Err(e),
        }

        let mut ml = map.write().expect("couldn't access writer");
        for event in self.events.iter() {
            println!("{:?}", event);
            self.dispatch(&mut ml, event);
        }
        Ok(())
    }
    pub fn dispatch(&self, ml: &mut HashMap<mio::Token, Scheduled>, e: mio::Event) {
        let kind = e.readiness();

        let io = match ml.get_mut(&e.token()) {
            Some(io) => io,
            None => return,
        };

        io.set_readiness(|old| old | kind.as_usize());

        if kind.is_writable() || mio::unix::UnixReady::from(kind).is_hup() {
            io.writer.wake();
        }
        if !(kind & (!mio::Ready::writable())).is_empty() {
            io.reader.wake();
        }
    }
}

impl Inner {
    pub fn add_io(&self, token: mio::Token, source: &dyn mio::Evented) {
        let Self { io, map, .. } = self;

        map.write().unwrap().insert(
            token.clone(),
            Scheduled {
                readiness: AtomicUsize::new(mio::Ready::empty().as_usize()),
                reader: AtomicWaker::new(),
                writer: AtomicWaker::new(),
            },
        );
        let _ = io.register(source, token, mio::Ready::all(), mio::PollOpt::edge());
        self.n_sources.fetch_add(1, Ordering::SeqCst);
    }
    pub fn deregister_source(&self, source: &dyn mio::Evented) -> io::Result<()> {
        self.io.deregister(source)
    }
    pub fn register(&self, token: mio::Token, dir: Direction, w: &Waker) {
        let rl = self.map.read().expect("couldn't acquire read access");

        let sched = rl
            .get(&token)
            .unwrap_or_else(|| panic!("IO resource for token {:?} does not exist!", token));

        let readiness = sched.get_readiness();

        let (waker, ready) = match dir {
            Direction::Read => (&sched.reader, !mio::Ready::writable()),
            Direction::Write => (&sched.writer, mio::Ready::writable()),
        };

        waker.register(w);

        if readiness & ready.as_usize() != 0 {
            waker.wake();
        }
    }
    pub fn drop_source(&self, token: &mio::Token) {
        drop(
            self.map
                .write()
                .expect("couldn't access write map ressource")
                .remove(token),
        );
        self.n_sources.fetch_sub(1, Ordering::SeqCst);
    }
    pub fn read_map(&self) -> std::sync::RwLockReadGuard<HashMap<mio::Token, Scheduled>> {
        self.map.read().expect("couldn't access the map")
    }
}

impl Handle {
    pub fn inner(&self) -> Option<Arc<Inner>> {
        self.inner.upgrade()
    }
}

impl Direction {
    pub fn mask(self) -> mio::Ready {
        match self {
            Self::Read => mio::Ready::all() - mio::Ready::writable(),
            Self::Write => mio::Ready::writable() | mio::unix::UnixReady::hup(),
        }
    }
}
