mod sinkable;
mod streamable;

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use sinkable::{sink, Sinkable};
use streamable::{stream, Streamable};

use bytes::BytesMut;
use futures::{
    io::{AsyncRead, AsyncWrite},
    sink::Sink,
    stream::Stream,
};
use pin_project_lite::pin_project;
use tokio_util::codec::{Decoder, Encoder, FramedParts};

pub(super) const INITIAL_CAPACITY: usize = 8 * 1024;
pub(super) const BACKPRESSURE_BOUNDARY: usize = INITIAL_CAPACITY;

pub(crate) trait ProjectFuse {
    type Io;
    type Codec;
    fn project(self: Pin<&mut Self>) -> Fused<Pin<&mut Self::Io>, &mut Self::Codec>;
}

pin_project! {
    pub struct Framed<T, U> {
        #[pin]
        inner: Streamable<Sinkable<Fused<T, U>>>,
    }
}
impl<T, U> Framed<T, U> {
    #[allow(dead_code)]
    pub fn new(io: T, codec: U) -> Self {
        Self {
            inner: stream(
                sink(
                    Fused { io, codec },
                    BytesMut::with_capacity(INITIAL_CAPACITY),
                ),
                BytesMut::with_capacity(INITIAL_CAPACITY),
            ),
        }
    }
    pub fn from_parts(parts: FramedParts<T, U>) -> Self {
        let FramedParts {
            io,
            codec,
            read_buf,
            write_buf,
            ..
        } = parts;
        Self {
            inner: stream(sink(Fused { io, codec }, write_buf), read_buf),
        }
    }
}

impl<T, U> Stream for Framed<T, U>
where
    T: AsyncRead,
    U: Decoder,
{
    type Item = Result<U::Item, U::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

impl<T, I, U> Sink<I> for Framed<T, U>
where
    T: AsyncWrite,
    U: Encoder<Item = I>,
    U::Error: From<io::Error>,
{
    type Error = U::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.get_pin_mut().poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        self.project().inner.get_pin_mut().start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.get_pin_mut().poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.get_pin_mut().poll_close(cx)
    }
}

pin_project! {
    pub(super) struct Fused<I, C> {
        #[pin]
        io: I,
        codec: C,
    }
}

impl<T, U> ProjectFuse for Fused<T, U> {
    type Io = T;
    type Codec = U;

    fn project(self: Pin<&mut Self>) -> Fused<Pin<&mut Self::Io>, &mut Self::Codec> {
        let self_ = self.project();
        Fused {
            io: self_.io,
            codec: self_.codec,
        }
    }
}
impl<I: AsyncRead, C> AsyncRead for Fused<I, C> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.project().io.poll_read(cx, buf)
    }
}
impl<I: AsyncWrite, C> AsyncWrite for Fused<I, C> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        self.project().io.poll_write(cx, buf)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().io.poll_flush(cx)
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().io.poll_close(cx)
    }
}
impl<I, C: Encoder> Encoder for Fused<I, C> {
    type Item = C::Item;
    type Error = C::Error;
    fn encode(&mut self, item: Self::Item, buf: &mut BytesMut) -> Result<(), Self::Error> {
        self.codec.encode(item, buf)
    }
}
impl<I, C: Decoder> Decoder for Fused<I, C> {
    type Item = C::Item;
    type Error = C::Error;
    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.codec.decode(buffer)
    }
    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.codec.decode_eof(buffer)
    }
}
