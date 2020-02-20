use std::{
    io,
    task::{Context, Poll},
};

use super::{Direction, Handle};

pub struct Registration {
    handle: Handle,
    token: mio::Token,
}

impl Registration {
    pub fn new(handle: Handle, token: mio::Token, io: &dyn mio::Evented) -> io::Result<Self> {
        let inner = match handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "driver gone")),
        };
        inner.add_io(token, io);
        Ok(Self { handle, token })
    }
    pub fn deregister(&mut self, io: &dyn mio::Evented) -> io::Result<()> {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "reactor gone")),
        };
        inner.deregister_source(io)
    }

    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<mio::Ready>> {
        self.poll_ready(Direction::Read, Some(cx))?.map(Ok)
    }
    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<mio::Ready>> {
        self.poll_ready(Direction::Write, Some(cx))?.map(Ok)
    }
    pub fn take_write_ready(&self) -> io::Result<Option<mio::Ready>> {
        match self.poll_ready(Direction::Write, None)? {
            Poll::Ready(ready) => Ok(Some(ready)),
            Poll::Pending => Ok(None),
        }
    }
    pub fn take_read_ready(&self) -> io::Result<Option<mio::Ready>> {
        match self.poll_ready(Direction::Read, None)? {
            Poll::Ready(ready) => Ok(Some(ready)),
            Poll::Pending => Ok(None),
        }
    }

    fn poll_ready(
        &self,
        direction: Direction,
        cx: Option<&mut Context<'_>>,
    ) -> io::Result<Poll<mio::Ready>> {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "reactor gone")),
        };

        if let Some(ref cx) = cx {
            inner.register(self.token, direction, cx.waker())
        }

        let mask = direction.mask();
        let mask_no_hup = (mask - mio::unix::UnixReady::hup()).as_usize();

        let rl = inner.read_map();
        let sched = match rl.get(&self.token) {
            Some(shed) => shed,
            None => return Err(io::Error::new(io::ErrorKind::Other, "token not found")),
        };

        let curr_ready = sched.set_readiness(|curr| curr & (!mask_no_hup));
        let mut ready = mask & mio::Ready::from_usize(curr_ready);

        if ready.is_empty() {
            if let Some(cx) = cx {
                // Update the task info
                match direction {
                    Direction::Read => sched.reader.register(cx.waker()),
                    Direction::Write => sched.writer.register(cx.waker()),
                }
                // Try again
                let curr_ready = sched.set_readiness(|curr| curr & (!mask_no_hup));
                ready = mask & mio::Ready::from_usize(curr_ready);
            }
        }

        if ready.is_empty() {
            Ok(Poll::Pending)
        } else {
            Ok(Poll::Ready(ready))
        }
    }
}

unsafe impl Send for Registration {}
unsafe impl Sync for Registration {}

impl Drop for Registration {
    fn drop(&mut self) {
        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return,
        };
        inner.drop_source(&self.token);
    }
}
