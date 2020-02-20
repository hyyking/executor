use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::{Buf, BytesMut};
use futures::{
    io::{AsyncRead, AsyncWrite},
    ready,
    sink::Sink,
};
use pin_project_lite::pin_project;
use tokio_util::codec::Encoder;

use super::{Fused, ProjectFuse, BACKPRESSURE_BOUNDARY};

pin_project! {
    pub(super) struct Sinkable<T> {
        #[pin]
        inner: T,
        buff: BytesMut,
    }
}

pub(super) fn sink<T>(inner: T, buff: BytesMut) -> Sinkable<T> {
    Sinkable { inner, buff }
}

impl<I, T> Sink<I> for Sinkable<T>
where
    T: ProjectFuse + AsyncWrite,
    T::Codec: Encoder<Item = I>,
{
    type Error = <T::Codec as Encoder>::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // If the buffer is already over 8KiB, then attempt to flush it. If after flushing it's
        // *still* over 8KiB, then apply backpressure (reject the send).
        if self.buff.len() >= BACKPRESSURE_BOUNDARY {
            match self.as_mut().poll_flush(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => (),
            };

            if self.buff.len() >= BACKPRESSURE_BOUNDARY {
                return Poll::Pending;
            }
        }
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        let mut pinned = self.project();
        pinned
            .inner
            .project()
            .codec
            .encode(item, &mut pinned.buff)?;
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut pinned = self.project();

        while !pinned.buff.is_empty() {
            let buf = &pinned.buff;
            let n = ready!(pinned.inner.as_mut().poll_write(cx, &buf))?;

            if n == 0 {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to \
                     write frame to transport",
                )
                .into()));
            }

            pinned.buff.advance(n);
        }

        // Try flushing the underlying IO
        ready!(pinned.inner.poll_flush(cx))?;

        Poll::Ready(Ok(()))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.as_mut().poll_flush(cx))?;
        ready!(self.project().inner.poll_close(cx))?;

        Poll::Ready(Ok(()))
    }
}
impl<T: AsyncRead> AsyncRead for Sinkable<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_read(cx, buf)
    }
}

impl<T> ProjectFuse for Sinkable<T>
where
    T: ProjectFuse,
{
    type Io = T::Io;
    type Codec = T::Codec;

    fn project(self: Pin<&mut Self>) -> Fused<Pin<&mut Self::Io>, &mut Self::Codec> {
        self.project().inner.project()
    }
}
