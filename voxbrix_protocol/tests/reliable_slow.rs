use futures_lite::{
    future,
    FutureExt,
};
use std::{
    future::Future,
    net::{
        SocketAddr,
        UdpSocket,
    },
    sync::{
        atomic::{
            AtomicU16,
            AtomicU64,
            Ordering,
        },
        Arc,
    },
    thread,
    time::Duration,
};
use tokio::time;
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

static TEST_NUM_DISPENCER: AtomicU16 = AtomicU16::new(2000);

struct ProxyPacketData {
    is_server: bool,
    packet_num: usize,
}

fn create_proxy<F>(
    test_num: u16,
    server_addr: SocketAddr,
    client_addr: SocketAddr,
    filter: F,
) -> SocketAddr
where
    F: Fn(ProxyPacketData) -> bool + Send + 'static,
{
    let port = 30000 + test_num * 10 + 2;

    let socket_addr: SocketAddr = ([127, 0, 0, 1], port).into();

    let socket = UdpSocket::bind(&socket_addr).unwrap();

    thread::spawn(move || {
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let mut packet_num_a = 0;
        let mut packet_num_b = 0;
        while let Ok((len, addr)) = socket.recv_from(&mut buf) {
            let (send_addr, packet_num) = match addr {
                addr if addr == server_addr => (client_addr, &mut packet_num_a),
                addr if addr == client_addr => (server_addr, &mut packet_num_b),
                _ => continue,
            };

            if filter(ProxyPacketData {
                is_server: addr == server_addr,
                packet_num: *packet_num,
            }) {
                socket.send_to(&buf[.. len], send_addr).unwrap();
            }

            *packet_num += 1;
        }
    });

    socket_addr
}

async fn reliability_test<'s, 'c>(
    mut server_check: impl AsyncFnMut(&mut server::Connection) + 's,
    mut client_check: impl AsyncFnMut(&mut client::Connection) + 'c,
    proxy_fn: impl Fn(ProxyPacketData) -> bool + Send + 'static,
) -> (impl Future<Output = ()> + 's, impl Future<Output = ()> + 'c) {
    let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

    let client_port = 30000 + test_num * 10 + 1;
    let server_port = 30000 + test_num * 10;

    let client_addr = ([127, 0, 0, 1], client_port);
    let server_addr = ([127, 0, 0, 1], server_port);

    let proxy_addr = create_proxy(test_num, server_addr.into(), client_addr.into(), proxy_fn);

    let mut server = ServerParameters::default()
        .bind(server_addr)
        .await
        .expect("server socket bind");

    let server_task = async move {
        let mut conn = server.accept().await.expect("connection accepted");

        async move {
            server_check(&mut conn).await;
        }
        .or(async {
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
    };

    (server_task, client_task)
}

#[tokio::test]
async fn reliable_test_reliability() {
    let _ = env_logger::try_init();

    let amount = 10;

    let server_check = async |conn: &mut server::Connection| {
        for i in 0 .. amount {
            conn.sender
                .send_reliable(format!("HelloWorld{}", i).as_bytes())
                .await
                .expect("server sent packet");
        }

        let ping_task = async {
            loop {
                time::sleep(Duration::from_millis(500)).await;
                conn.sender
                    .send_reliable("serv_ping".as_bytes())
                    .await
                    .expect("server sent packet");
            }
        };

        let recv_task = async {
            for i in 0 .. amount {
                let msg = conn.receiver.recv().await.expect("client message receive");
                assert_eq!(msg.data().as_ref(), format!("HelloWorld{}", i).as_bytes());
            }
        };

        recv_task.or(ping_task).await;
    };

    let client_check = async |conn: &mut client::Connection| {
        for i in 0 .. amount {
            let msg = conn.receiver.recv().await.expect("client message receive");
            assert_eq!(msg.data(), format!("HelloWorld{}", i).as_bytes());
        }

        let send_task = async {
            for i in 0 .. amount {
                conn.sender
                    .send_reliable(format!("HelloWorld{}", i).as_bytes())
                    .await
                    .expect("server sent packet");
            }

            loop {
                time::sleep(Duration::from_millis(500)).await;
                conn.sender
                    .send_reliable("cl_ping".as_bytes())
                    .await
                    .expect("server sent packet");
            }
        };

        let ack_task = async {
            loop {
                let _ = conn.receiver.recv().await;
            }
        };

        send_task.or(ack_task).await;
    };

    let (server_task, client_task) =
        reliability_test(server_check, client_check, |d| d.packet_num % 3 != 2).await;

    server_task.or(client_task).await;
}

#[tokio::test]
async fn reliable_test_redundancy() {
    let _ = env_logger::try_init();

    let amount = 1210;
    let missed_packets = 20;

    let packet_counter = Arc::new(AtomicU64::new(0));

    let server_check = async |conn: &mut server::Connection| {
        for i in 0 .. amount {
            conn.sender
                .send_reliable(format!("HelloWorld{}", i).as_bytes())
                .await
                .expect("server sent packet");
        }

        conn.sender.wait_complete().await.expect("wait complete");

        for i in 0 .. amount {
            let msg = conn.receiver.recv().await.expect("client message receive");
            assert_eq!(msg.data().as_ref(), format!("HelloWorld{}", i).as_bytes());
        }
    };

    let client_check = async |conn: &mut client::Connection| {
        for i in 0 .. amount {
            let msg = conn.receiver.recv().await.expect("client message receive");
            assert_eq!(msg.data(), format!("HelloWorld{}", i).as_bytes());
        }

        let send_task = async {
            for i in 0 .. amount {
                conn.sender
                    .send_reliable(format!("HelloWorld{}", i).as_bytes())
                    .await
                    .expect("server sent packet");
            }

            conn.sender.wait_complete().await.expect("wait complete");
        };

        send_task
            .or(async {
                let _ = conn.receiver.recv().await;
            })
            .await;
    };

    let (server_task, client_task) = reliability_test(server_check, client_check, {
        let packet_counter = packet_counter.clone();
        move |d| {
            if d.is_server {
                packet_counter.fetch_add(1, Ordering::Relaxed);

                for j in 1 ..= missed_packets {
                    if d.packet_num == j * 10 {
                        return false;
                    }
                }
            }

            true
        }
    })
    .await;

    future::zip(server_task, client_task).await;

    assert_eq!(
        // 1 is ACCEPT;
        // DISCONNECT not sent due to neigher end being dropped.
        amount * 2 + 1 + missed_packets as u64,
        packet_counter.load(Ordering::Relaxed)
    );
}
