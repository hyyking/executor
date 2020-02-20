use std::{
    io::{self, Read, Write},
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use crate::io::{Handle, PollEvented};

use futures::{
    io::{AsyncRead, AsyncWrite},
    ready,
};

use tokio::io::{AsyncRead as TokioAsyncRead, AsyncWrite as TokioAsyncWrite};

pub struct TcpStream {
    io: PollEvented<mio::net::TcpStream>,
}

impl TcpStream {
    pub fn new(handle: Handle, connected: mio::net::TcpStream) -> io::Result<TcpStream> {
        let port = connected.peer_addr()?.port() as usize;
        let io = PollEvented::new(handle, mio::Token(port), connected)?;
        Ok(TcpStream { io })
    }

    pub async fn connect(handle: Handle, addr: SocketAddr) -> io::Result<Self> {
        let sys = mio::net::TcpStream::connect(&addr)?;
        let stream = Self::new(handle, sys)?;

        futures::future::poll_fn(|cx| stream.io.poll_write_ready(cx)).await?;

        if let Some(e) = stream.io.get_ref().take_error()? {
            return Err(e);
        }

        Ok(stream)
    }
}

impl AsyncRead for TcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;

        match self.io.get_ref().read(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_write_ready(cx))?;

        match self.io.get_ref().write(buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }
    #[inline]
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.io.get_ref().shutdown(std::net::Shutdown::Write)?;
        Poll::Ready(Ok(()))
    }
}

impl TokioAsyncRead for TcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        <Self as AsyncRead>::poll_read(self, cx, buf)
    }
}

impl TokioAsyncWrite for TcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<Result<usize, tokio::io::Error>> {
        <Self as AsyncWrite>::poll_write(self, cx, buf)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), tokio::io::Error>> {
        <Self as AsyncWrite>::poll_flush(self, cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), tokio::io::Error>> {
        <Self as AsyncWrite>::poll_close(self, cx)
    }
}
