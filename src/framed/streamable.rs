use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bytes::BytesMut;
use futures::{io::AsyncRead, stream::Stream};
use pin_project_lite::pin_project;
use tokio_util::codec::Decoder;

use super::ProjectFuse;

pin_project! {
    pub(super) struct Streamable<T> {
        #[pin]
        inner: T,
        is_eof: bool,
        is_readable: bool,
        buff: BytesMut,
    }
}

pub(super) fn stream<T>(inner: T, buff: BytesMut) -> Streamable<T> {
    Streamable {
        inner,
        is_eof: false,
        is_readable: false,
        buff,
    }
}

impl<T> Streamable<T> {
    pub(crate) fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut T> {
        self.project().inner
    }
}

impl<T> Stream for Streamable<T>
where
    T: ProjectFuse + AsyncRead,
    T::Codec: Decoder,
{
    type Item = Result<<T::Codec as Decoder>::Item, <T::Codec as Decoder>::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut pinned = self.project();
        loop {
            if *pinned.is_readable {
                if *pinned.is_eof {
                    let frame = pinned.inner.project().codec.decode_eof(&mut pinned.buff)?;
                    return Poll::Ready(frame.map(Ok));
                }

                if let Some(frame) = pinned
                    .inner
                    .as_mut()
                    .project()
                    .codec
                    .decode(&mut pinned.buff)?
                {
                    return Poll::Ready(Some(Ok(frame)));
                }

                *pinned.is_readable = false;
            }

            assert!(!*pinned.is_eof);

            // Otherwise, try to read more data and try again. Make sure we've
            // got room for at least one byte to read to ensure that we don't
            // get a spurious 0 that looks like EOF
            pinned.buff.reserve(1);
            let bytect = match pinned.inner.as_mut().poll_read(cx, &mut pinned.buff)? {
                Poll::Ready(ct) => ct,
                Poll::Pending => return Poll::Pending,
            };
            if bytect == 0 {
                *pinned.is_eof = true;
            }

            *pinned.is_readable = true;
        }
    }
}
