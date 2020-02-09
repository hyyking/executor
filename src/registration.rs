use crate::core::{Driver, Handle};
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};

pub struct Registration {
    handle: Handle,
    token: mio::Token,
}

impl Registration {
    pub fn new(d: &Driver, token: mio::Token, io: &dyn mio::Evented) -> Self {
        let handle = d.handle();
        d.add_io(token, io);
        Self { handle, token }
    }
    pub fn poll_ready(&self, cx: Option<&mut Context<'_>>) -> Poll<mio::Ready> {
        let rl = self.handle.read().unwrap();
        let io = match rl.get(&self.token) {
            Some(io) => io,
            None => return Poll::Pending,
        };

        let status = mio::Ready::from_usize(io.readiness.load(Ordering::SeqCst));

        if let Some(cx) = cx {
            io.writer.register(cx.waker());
            io.reader.register(cx.waker());
        }

        if status == mio::Ready::empty() {
            Poll::Pending
        } else {
            io.readiness
                .store(mio::Ready::empty().as_usize(), Ordering::SeqCst);

            Poll::Ready(status)
        }
    }
}
