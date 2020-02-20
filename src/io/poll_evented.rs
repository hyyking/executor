use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use super::{Handle, Registration};

use futures::{
    io::{AsyncRead, AsyncWrite},
    ready,
};

pub struct PollEvented<E: mio::Evented> {
    io: Option<E>,
    inner: Inner,
}

struct Inner {
    registration: Registration,
    read_readiness: AtomicUsize,
    write_readiness: AtomicUsize,
}

macro_rules! poll_ready {
    ($me:expr, $mask:expr, $cache:ident, $take:ident, $poll:expr) => {{
        // Load cached & encoded readiness.
        let mut cached = $me.inner.$cache.load(Ordering::Relaxed);
        let mask = $mask | mio::unix::UnixReady::hup();

        // See if the current readiness matches any bits.
        let mut ret = mio::Ready::from_usize(cached) & $mask;

        if ret.is_empty() {
            // Readiness does not match, consume the registration's readiness
            // stream. This happens in a loop to ensure that the stream gets
            // drained.
            loop {
                let ready = match $poll? {
                    Poll::Ready(v) => v,
                    Poll::Pending => return Poll::Pending,
                };
                cached |= ready.as_usize();

                // Update the cache store
                $me.inner.$cache.store(cached, Ordering::Relaxed);

                ret |= ready & mask;

                if !ret.is_empty() {
                    return Poll::Ready(Ok(ret));
                }
            }
        } else {
            // Check what's new with the registration stream. This will not
            // request to be notified
            if let Some(ready) = $me.inner.registration.$take()? {
                cached |= ready.as_usize();
                $me.inner.$cache.store(cached, Ordering::Relaxed);
            }

            Poll::Ready(Ok(mio::Ready::from_usize(cached)))
        }
    }};
}

impl<E: mio::Evented> PollEvented<E> {
    pub fn new(handle: Handle, token: mio::Token, io: E) -> io::Result<Self> {
        let registration = Registration::new(handle, token, &io)?;
        Ok(Self {
            io: Some(io),
            inner: Inner {
                registration,
                read_readiness: AtomicUsize::new(0),
                write_readiness: AtomicUsize::new(0),
            },
        })
    }

    pub fn get_mut(&mut self) -> &mut E {
        self.io.as_mut().unwrap()
    }
    pub fn get_ref(&self) -> &E {
        self.io.as_ref().unwrap()
    }

    pub fn poll_read_ready(
        &self,
        cx: &mut Context<'_>,
        mask: mio::Ready,
    ) -> Poll<io::Result<mio::Ready>> {
        assert!(!mask.is_writable(), "cannot poll for write readiness");
        poll_ready!(
            self,
            mask,
            read_readiness,
            take_read_ready,
            self.inner.registration.poll_read_ready(cx)
        )
    }
    pub fn clear_read_ready(&self, cx: &mut Context<'_>, ready: mio::Ready) -> io::Result<()> {
        // Cannot clear write readiness
        assert!(!ready.is_writable(), "cannot clear write readiness");
        assert!(
            !mio::unix::UnixReady::from(ready).is_hup(),
            "cannot clear HUP readiness"
        );

        self.inner
            .read_readiness
            .fetch_and(!ready.as_usize(), Ordering::Relaxed);

        if self.poll_read_ready(cx, ready)?.is_ready() {
            cx.waker().wake_by_ref();
        }

        Ok(())
    }

    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<mio::Ready>> {
        poll_ready!(
            self,
            mio::Ready::writable(),
            write_readiness,
            take_write_ready,
            self.inner.registration.poll_write_ready(cx)
        )
    }
    pub fn clear_write_ready(&self, cx: &mut Context<'_>) -> io::Result<()> {
        self.inner
            .write_readiness
            .fetch_and(!mio::Ready::writable().as_usize(), Ordering::Relaxed);

        if self.poll_write_ready(cx)?.is_ready() {
            cx.waker().wake_by_ref();
        }

        Ok(())
    }
}

impl<E> AsyncRead for PollEvented<E>
where
    E: mio::Evented + std::io::Read + std::marker::Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        println!("evented poll_read");
        ready!(self.poll_read_ready(cx, mio::Ready::readable()))?;

        let r = (*self).get_mut().read(buf);

        if is_wouldblock(&r) {
            self.clear_read_ready(cx, mio::Ready::readable())?;
            return Poll::Pending;
        }
        Poll::Ready(r)
    }
}

impl<E> AsyncWrite for PollEvented<E>
where
    E: mio::Evented + std::io::Write + std::marker::Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.poll_write_ready(cx))?;

        let r = (*self).get_mut().write(buf);

        if is_wouldblock(&r) {
            self.clear_write_ready(cx)?;
            return Poll::Pending;
        }

        Poll::Ready(r)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.poll_write_ready(cx))?;

        let r = (*self).get_mut().flush();

        if is_wouldblock(&r) {
            self.clear_write_ready(cx)?;
            return Poll::Pending;
        }

        Poll::Ready(r)
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn is_wouldblock<T>(r: &io::Result<T>) -> bool {
    match *r {
        Ok(_) => false,
        Err(ref e) => e.kind() == io::ErrorKind::WouldBlock,
    }
}

impl<E: mio::Evented> Drop for PollEvented<E> {
    fn drop(&mut self) {
        if let Some(io) = self.io.take() {
            let _ = self.inner.registration.deregister(&io);
        }
    }
}
