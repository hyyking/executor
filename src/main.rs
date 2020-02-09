mod core;
mod registration;

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    thread,
};

use crate::core::Driver;
use crate::registration::Registration;

use mio::{
    net::{TcpListener, TcpStream},
    Token,
};

struct PollEvented<T> {
    io: T,
    reg: Registration,
}

impl<T> Future for PollEvented<T> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Poll::Ready(s) = self.reg.poll_ready(Some(cx)) {
            if s == mio::Ready::writable() {
                println!("WRITABLE");
                return Poll::Ready(());
            } else if s == mio::Ready::readable() {
                println!("READABLE");
                return Poll::Ready(());
            }
        }
        println!("PENDING");
        Poll::Pending
    }
}

fn main() {
    // Setup some tokens to allow us to identify which event is
    // for which socket.
    const SERVER: Token = Token(0);
    const CLIENT: Token = Token(1);

    let addr = "127.0.0.1:13265".parse().unwrap();

    // Setup the driver
    let mut core = Driver::new();

    let server = TcpListener::bind(&addr).unwrap();
    core.add_io(SERVER, &server);

    // Setup the client socket
    let client = TcpStream::connect(&addr).unwrap();
    let reg = core.register(CLIENT, &client);

    let a = PollEvented { io: client, reg };

    let h = thread::spawn(move || loop {
        assert!(core.turn().is_ok());
    });

    futures::executor::block_on(a);

    let _ = h.join();
}
