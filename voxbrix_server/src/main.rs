use anyhow::{
    Error,
    Result,
};
use async_executor::LocalExecutor;
use futures_lite::future;
use voxbrix_messages::{
    client::ClientAccept,
    server::ServerAccept,
    Pack,
};
use voxbrix_protocol::server::{
    Server,
    StreamReceiver,
    StreamSender,
};

async fn handle(mut tx: StreamSender, mut rx: StreamReceiver) {
    let mut send_buf = Vec::new();

    while let Ok((channel, msg)) = rx.recv().await {
        let msg = match ServerAccept::unpack(msg) {
            Ok(m) => m,
            Err(_) => continue,
        };

        match msg {
            ServerAccept::GetChunksBlocks { coords } => {
                for chunk in coords {
                    let response = if chunk.position[2] < 0 {
                        ClientAccept::ClassBlockComponent {
                            coords: chunk,
                            value: vec![1; 4096],
                        }
                    } else {
                        let mut value = vec![0; 4096];
                        value[chunk.position[0].abs() as usize * 2
                            + chunk.position[1].abs() as usize] = 1;
                        ClientAccept::ClassBlockComponent {
                            coords: chunk,
                            value,
                        }
                    };

                    // TODO: handle error
                    response.pack(&mut send_buf).expect("message pack");

                    // TODO handle retry
                    tx.send_reliable(channel, &send_buf)
                        .await
                        .expect("message send");
                }
            },
        }
    }
}

fn main() -> Result<()> {
    let rt = LocalExecutor::new();
    future::block_on(rt.run(async {
        let mut server = Server::bind(([127, 0, 0, 1], 12000))?;

        while let Ok((tx, rx)) = server.accept().await {
            rt.spawn(handle(tx, rx)).detach();
        }

        Ok::<(), Error>(())
    }))?;

    Ok(())
}
