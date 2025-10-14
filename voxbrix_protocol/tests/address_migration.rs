use futures_lite::{
    future,
    FutureExt,
};
use std::{
    net::SocketAddr,
    sync::{
        atomic::{
            AtomicU16,
            Ordering,
        },
        Arc,
        Mutex,
    },
    time::Duration,
};
use tokio::{
    net::UdpSocket,
    time,
};
use voxbrix_protocol::{
    client::{
        self,
        Client,
    },
    server::{
        self,
        ServerParameters,
    },
    MAX_PACKET_SIZE,
};

static TEST_NUM_DISPENCER: AtomicU16 = AtomicU16::new(3000);

// We create a proxy that initially uses another socket address to resend messages,
// but after switch works as a basic proxy on a single socket address.
// This mocks the required effect of "client changed address".
async fn create_proxy_switchable(
    test_num: u16,
    peer_a: SocketAddr,
    peer_b: SocketAddr,
) -> (SocketAddr, SocketAddr, Arc<Mutex<bool>>) {
    let port_1 = 30000 + test_num * 10 + 2;
    let port_2 = 30000 + test_num * 10 + 3;

    let socket_addr_1: SocketAddr = ([127, 0, 0, 1], port_1).into();
    let socket_addr_2: SocketAddr = ([127, 0, 0, 1], port_2).into();

    let socket_1 = UdpSocket::bind(&socket_addr_1).await.unwrap();
    let mut socket_2 = Some(UdpSocket::bind(&socket_addr_2).await.unwrap());

    let switch = Arc::new(Mutex::new(true));

    let switch_inner = switch.clone();

    tokio::spawn(async move {
        loop {
            if !*switch_inner.lock().unwrap() {
                // Drop second proxy so it cannot be used again
                socket_2.take();
            }

            let mut buf_1 = [0u8; MAX_PACKET_SIZE];
            let mut buf_2 = [0u8; MAX_PACKET_SIZE];

            let ((len, addr), first) =
                async { Ok::<_, std::io::Error>((socket_1.recv_from(&mut buf_1).await?, true)) }
                    .or(async {
                        if let Some(socket_2) = socket_2.as_ref() {
                            Ok((socket_2.recv_from(&mut buf_2).await?, false))
                        } else {
                            future::pending().await
                        }
                    })
                    .await
                    .unwrap();

            let data = if first {
                &buf_1[.. len]
            } else {
                &buf_2[.. len]
            };

            let send_to = if addr == peer_a {
                peer_b
            } else if addr == peer_b {
                peer_a
            } else {
                continue;
            };

            let send_from = if *switch_inner.lock().unwrap() {
                if send_to == peer_a {
                    &socket_1
                } else if send_to == peer_b {
                    socket_2.as_ref().unwrap()
                } else {
                    panic!()
                }
            } else {
                &socket_1
            };

            send_from.send_to(data, send_to).await.unwrap();
        }
    });

    (socket_addr_1, socket_addr_2, switch)
}

async fn address_migration_test(
    mut server_check: impl AsyncFnMut(&mut server::Connection),
    mut client_check: impl AsyncFnMut(&mut client::Connection),
) {
    let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

    let client_port = 30000 + test_num * 10 + 1;
    let server_port = 30000 + test_num * 10;

    let client_addr = ([127, 0, 0, 1], client_port);
    let server_addr = ([127, 0, 0, 1], server_port);

    let (proxy_addr, _, switch) =
        create_proxy_switchable(test_num, client_addr.into(), server_addr.into()).await;

    let mut server = ServerParameters::default()
        .bind(server_addr)
        .await
        .expect("server socket bind");

    let server_task = async move {
        let mut conn = server.accept().await.expect("connection accepted");

        async move {
            server_check(&mut conn).await;
            // Here the switch happens
            server_check(&mut conn).await;
        }
        .or(async move {
            loop {
                server.accept().await.unwrap();
            }
        })
        .await
    };

    let client_task = async move {
        time::sleep(Duration::from_millis(5)).await;

        let client = Client::bind(client_addr).await.expect("client bound");

        let mut conn = client.connect(proxy_addr).await.expect("client connection");

        client_check(&mut conn).await;

        *switch.lock().unwrap() = false;

        client_check(&mut conn).await;
    };

    future::zip(server_task, client_task).await;
}

// Tests that server will set a new address for a client if a confirmed unreliable
// message comes from this new address.
#[tokio::test]
async fn unreliable_address_migration() {
    let _ = env_logger::try_init();

    let amount = 10;

    let server_check = async move |conn: &mut server::Connection| {
        for i in 0 .. amount {
            let msg = conn.receiver.recv().await.expect("server message receive");
            assert_eq!(msg.data().as_ref(), format!("FromClient{}", i).as_bytes());
        }

        for i in 0 .. amount {
            conn.sender
                .send_unreliable(format!("FromServer{}", i).as_bytes())
                .await
                .expect("server sent packet");
        }
    };

    let client_check = async move |conn: &mut client::Connection| {
        for i in 0 .. amount {
            conn.sender
                .send_unreliable(format!("FromClient{}", i).as_bytes())
                .await
                .expect("server sent packet");
        }

        for i in 0 .. amount {
            let msg = conn.receiver.recv().await.expect("client message receive");
            assert_eq!(msg.data().as_ref(), format!("FromServer{}", i).as_bytes());
        }
    };

    address_migration_test(server_check, client_check).await;
}

// Tests that server will set a new address for a client if a confirmed reliable
// message comes from this new address.
#[tokio::test]
async fn reliable_address_migration() {
    let _ = env_logger::try_init();

    let amount = 10;

    let server_check = async move |conn: &mut server::Connection| {
        for i in 0 .. amount {
            let msg = conn.receiver.recv().await.expect("server message receive");
            assert_eq!(msg.data().as_ref(), format!("FromClient{}", i).as_bytes());
        }

        for i in 0 .. amount {
            conn.sender
                .send_reliable(format!("FromServer{}", i).as_bytes())
                .await
                .expect("server sent packet");
        }
    };

    let client_check = async move |conn: &mut client::Connection| {
        future::zip(
            async {
                for i in 0 .. amount {
                    conn.sender
                        .send_reliable(format!("FromClient{}", i).as_bytes())
                        .await
                        .expect("server sent packet");
                }
            },
            async {
                for i in 0 .. amount {
                    let msg = conn.receiver.recv().await.expect("client message receive");
                    assert_eq!(msg.data().as_ref(), format!("FromServer{}", i).as_bytes());
                }
            },
        )
        .await;
    };

    address_migration_test(server_check, client_check).await;
}
