mod framed;
mod io;
mod tcp;

use std::thread;

use crate::framed::Framed;
use crate::io::Driver;
use crate::tcp::TcpStream;

use futures::stream::StreamExt;

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

    let stream = futures::executor::block_on(TcpStream::connect(handle, addr))?;

    let client = ClientBuilder::new("ws://127.0.0.1:5000/sc2api").unwrap();

    let mut framed = Framed::from_parts(
        futures::executor::block_on(client.async_connect_on(stream))
            .unwrap()
            .into_parts(),
    );

    loop {
        thread::sleep(std::time::Duration::from_millis(1000));
        let c = futures::executor::block_on(framed.next());
        println!("{:?}", c);
    }
}
