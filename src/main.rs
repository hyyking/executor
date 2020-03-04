mod executor;
mod framed;
mod io;
mod macros;
mod park;
mod tcp;
/*
use std::thread;

use crate::executor::ThreadPool;
use crate::io::Driver;
use crate::park::Parker;
use crate::tcp::TcpStream;

use futures::{SinkExt, StreamExt};
use rsc2_pb::{
    api::{Request, RequestPing},
    codec::from_ws_client,
};
use websocket_lite::ClientBuilder;
*/

fn main() {}

/*
fn _main() -> Result<(), std::io::Error> {
    let addr = "127.0.0.1:5000".parse().unwrap();

    // Setup the driver
    let core = Driver::new();
    let handle = core.handle();

    let mut rt = Executor::new(Parker::new(core));

    let stream = rt.block_on(TcpStream::connect(handle, addr))?;
    let client = ClientBuilder::new("ws://127.0.0.1:5000/sc2api").unwrap();

    rt.block_on(async {
        let mut framed = from_ws_client(client.async_connect_on(stream).await.unwrap());
        for req in 0..5 {
            thread::sleep(std::time::Duration::from_secs(1));

            println!("SENDING...");
            let _ = framed.send(Request::new(req, RequestPing {})).await;
            println!("SENT...");
            let c = framed.next().await;
            println!("RECEIVED: {:#?}", c);
        }
    });

    Ok(())
}
*/
