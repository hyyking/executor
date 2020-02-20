mod block_on;
mod framed;
mod io;
mod tcp;

use std::thread;

use crate::block_on::block_on;
use crate::framed::Framed;
use crate::io::Driver;
use crate::tcp::TcpStream;

use futures::{SinkExt, StreamExt};
use rsc2_pb::{
    api::{Request, RequestPing},
    codec::from_ws_client,
};
use websocket_lite::ClientBuilder;

fn main() -> Result<(), std::io::Error> {
    let addr = "127.0.0.1:5000".parse().unwrap();

    // Setup the driver
    let mut core = Driver::new();
    let handle = core.handle();
    // Setup the client socket
    let _h = thread::spawn(move || loop {
        assert!(core.turn().is_ok());
        if core.empty() {
            break;
        }
    });

    let stream = block_on(TcpStream::connect(handle, addr))?;

    let client = ClientBuilder::new("ws://127.0.0.1:5000/sc2api").unwrap();

    block_on(async {
        let mut framed = from_ws_client(client.async_connect_on(stream).await.unwrap());
        for req in 0..5 {
            thread::sleep(std::time::Duration::from_secs(1));

            println!("SENDING...");
            framed.send(Request::new(req, RequestPing {})).await;
            println!("SENT...");
            let c = framed.next().await;
            println!("RECEIVED: {:#?}", c);
        }
    });
    Ok(())
}
