use std::{
    collections::HashMap,
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        RwLock, {Arc, Weak},
    },
    task::Waker,
};

use super::{Registration, Scheduled /* SuperSlab */};
use crate::park::{Park, Unpark};

use futures::task::AtomicWaker;

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum Direction {
    Read,
    Write,
}

pub struct Driver {
    events: mio::Events,
    inner: Arc<Inner>,
    _self_registration: mio::Registration,
}

#[derive(Clone)]
pub struct Handle {
    inner: Weak<Inner>,
}

pub struct Inner {
    io: mio::Poll,
    map: RwLock<HashMap<mio::Token, Scheduled>>,
    n_sources: AtomicUsize,
    self_wakeup: mio::SetReadiness,
}

impl Driver {
    const TOKEN: mio::Token = mio::Token(666);

    pub fn new() -> Self {
        let (_self_registration, self_wakeup) = mio::Registration::new2();
        let io = mio::Poll::new().expect("couldn't contruct mio::Poll");

        io.register(
            &_self_registration,
            Self::TOKEN,
            mio::Ready::readable(),
            mio::PollOpt::level(),
        )
        .unwrap();

        Self {
            events: mio::Events::with_capacity(1024),
            inner: Arc::new(Inner {
                map: RwLock::new(HashMap::new()),
                n_sources: AtomicUsize::new(0),
                io,
                self_wakeup,
            }),
            _self_registration,
        }
    }
    #[allow(dead_code)]
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
    pub fn turn(&mut self, timeout: Option<std::time::Duration>) -> io::Result<()> {
        let Inner { io, map, .. } = &*self.inner;

        match io.poll(&mut self.events, timeout) {
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
        let token = e.token();
        if token == Driver::TOKEN {
            self.inner
                .self_wakeup
                .set_readiness(mio::Ready::empty())
                .unwrap();
            return;
        }
        let io = match ml.get_mut(&token) {
            Some(io) => io,
            None => return,
        };

        let kind = e.readiness();
        io.set_readiness(|old| old | kind.as_usize());

        if kind.is_writable() || mio::unix::UnixReady::from(kind).is_hup() {
            io.writer.wake()
        }
        if !(kind & (!mio::Ready::writable())).is_empty() {
            io.reader.wake()
        }
    }
}

impl Inner {
    pub fn add_io(&self, token: mio::Token, source: &dyn mio::Evented) {
        let Self { io, map, .. } = &self;
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

    pub fn deregister_source(&self, source: &dyn mio::Evented) -> io::Result<()> {
        self.io.deregister(source)
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

impl Park for Driver {
    type Handle = Handle;
    fn handle(&self) -> Self::Handle {
        self.handle()
    }
    fn park(&mut self) -> io::Result<()> {
        self.turn(None)?;
        Ok(())
    }
    fn park_timeout(&mut self, dur: std::time::Duration) -> io::Result<()> {
        self.turn(Some(dur))?;
        Ok(())
    }
}

impl Handle {
    pub fn inner(&self) -> Option<Arc<Inner>> {
        self.inner.upgrade()
    }

    fn wake(&self) {
        if let Some(inner) = self.inner() {
            inner
                .self_wakeup
                .set_readiness(mio::Ready::readable())
                .unwrap();
        }
    }
}

impl Unpark for Handle {
    fn unpark(&self) {
        self.wake()
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
