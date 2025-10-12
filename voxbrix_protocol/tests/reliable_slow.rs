use std::{
    cell::RefCell,
    net::{
        SocketAddr,
        UdpSocket,
    },
    sync::atomic::{
        AtomicU16,
        AtomicU64,
        Ordering,
    },
    thread,
    time::Duration,
};
use tokio::{
    task::{
        self,
        LocalSet,
    },
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

static TEST_NUM_DISPENCER: AtomicU16 = AtomicU16::new(2000);

fn create_proxy<F>(test_num: u16, peer_a: SocketAddr, peer_b: SocketAddr, filter: F) -> SocketAddr
where
    F: Fn(usize, SocketAddr) -> bool + Send + 'static,
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
                addr if addr == peer_a => (peer_b, &mut packet_num_a),
                addr if addr == peer_b => (peer_a, &mut packet_num_b),
                _ => continue,
            };

            if filter(*packet_num, addr) {
                socket.send_to(&buf[.. len], send_addr).unwrap();
            }

            *packet_num += 1;
        }
    });

    socket_addr
}

#[tokio::test]
async fn reliable_test_reliability() {
    let _ = env_logger::try_init();

    let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

    let amount = 1000;

    let client_port = 30000 + test_num * 10 + 1;
    let server_port = 30000 + test_num * 10;

    let client_addr = ([127, 0, 0, 1], client_port);
    let server_addr = ([127, 0, 0, 1], server_port);

    let proxy_addr = create_proxy(
        test_num,
        client_addr.into(),
        server_addr.into(),
        |i, _addr| i % 3 != 2,
    );

    let task: &_ = Box::leak(Box::new(RefCell::new(None)));
    LocalSet::new()
        .run_until(async move {
            task::spawn_local(async move {
                let mut server = ServerParameters::default()
                    .bind(server_addr)
                    .await
                    .expect("server socket bind");

                loop {
                    let server::Connection {
                        sender: tx,
                        receiver: rx,
                        ..
                    } = server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(task::spawn_local(async move {
                        let mut tx = tx;
                        let mut rx = rx;
                        for i in 0 .. amount {
                            tx.send_reliable(format!("HelloWorld{}", i).as_bytes())
                                .await
                                .expect("server sent packet");
                        }

                        task::spawn_local(async {
                            let mut tx = tx;
                            loop {
                                time::sleep(Duration::from_millis(500)).await;
                                tx.send_reliable("serv_ping".as_bytes())
                                    .await
                                    .expect("server sent packet");
                            }
                        });

                        for i in 0 .. amount {
                            let msg = rx.recv().await.expect("client message receive");
                            assert_eq!(msg.data().as_ref(), format!("HelloWorld{}", i).as_bytes());
                        }
                    }));
                }
            });

            time::sleep(Duration::from_millis(5)).await;

            let client = Client::bind(client_addr).await.expect("client bound");

            let client::Connection {
                sender: mut tx,
                receiver: mut rx,
                ..
            } = client.connect(proxy_addr).await.expect("client connection");

            for i in 0 .. amount {
                let msg = rx.recv().await.expect("client message receive");
                assert_eq!(msg.data(), format!("HelloWorld{}", i).as_bytes());
            }

            task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

            for i in 0 .. amount {
                tx.send_reliable(format!("HelloWorld{}", i).as_bytes())
                    .await
                    .expect("server sent packet");
            }

            task::spawn_local(async {
                let mut tx = tx;
                loop {
                    time::sleep(Duration::from_millis(500)).await;
                    tx.send_reliable("cl_ping".as_bytes())
                        .await
                        .expect("server sent packet");
                }
            });

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;
}

#[tokio::test]
async fn reliable_test_redundancy() {
    let _ = env_logger::try_init();

    let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

    // Should resend only the packages that were not delivered

    let amount = 1210;
    let missed_packets = 20;

    let client_port = 30000 + test_num * 10 + 1;
    let server_port = 30000 + test_num * 10;

    let client_addr: SocketAddr = ([127, 0, 0, 1], client_port).into();
    let server_addr: SocketAddr = ([127, 0, 0, 1], server_port).into();

    let packet_num: &'static _ = Box::leak(Box::new(AtomicU64::new(0)));

    let proxy_addr = create_proxy(test_num, client_addr, server_addr, move |i, addr| {
        if addr == server_addr {
            packet_num.fetch_add(1, Ordering::Relaxed);

            for j in 1 ..= missed_packets {
                if i == j * 10 {
                    return false;
                }
            }
        }

        true
    });

    let task: &_ = Box::leak(Box::new(RefCell::new(None)));
    LocalSet::new()
        .run_until(async move {
            task::spawn_local(async move {
                let mut server = ServerParameters::default()
                    .bind(server_addr)
                    .await
                    .expect("server socket bind");

                loop {
                    let server::Connection {
                        sender: tx,
                        receiver: rx,
                        ..
                    } = server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(task::spawn_local(async move {
                        let mut tx = tx;
                        let mut rx = rx;
                        for i in 0 .. amount {
                            tx.send_reliable(format!("HelloWorld{}", i).as_bytes())
                                .await
                                .expect("server sent packet");
                        }

                        task::spawn_local(async move { tx.wait_complete().await });

                        for i in 0 .. amount {
                            let msg = rx.recv().await.expect("client message receive");
                            assert_eq!(msg.data().as_ref(), format!("HelloWorld{}", i).as_bytes());
                        }
                    }));
                }
            });

            time::sleep(Duration::from_millis(5)).await;

            let client = Client::bind(client_addr).await.expect("client bound");

            let client::Connection {
                sender: mut tx,
                receiver: mut rx,
                ..
            } = client.connect(proxy_addr).await.expect("client connection");

            for i in 0 .. amount {
                let msg = rx.recv().await.expect("client message receive");
                assert_eq!(msg.data(), format!("HelloWorld{}", i).as_bytes());
            }

            task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

            for i in 0 .. amount {
                tx.send_reliable(format!("HelloWorld{}", i).as_bytes())
                    .await
                    .expect("server sent packet");
            }

            task::spawn_local(async move { tx.wait_complete().await });

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;

    assert_eq!(
        amount * 2 + 2 + missed_packets as u64,
        packet_num.load(Ordering::Relaxed)
    );
}

#[tokio::test]
async fn reliable_test_wait_complete() {
    let _ = env_logger::try_init();

    let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

    let amount = 1000;

    let client_port = 30000 + test_num * 10 + 1;
    let server_port = 30000 + test_num * 10;

    let client_addr = ([127, 0, 0, 1], client_port);
    let server_addr = ([127, 0, 0, 1], server_port);

    let proxy_addr = create_proxy(
        test_num,
        client_addr.into(),
        server_addr.into(),
        |i, _addr| i % 3 != 2,
    );

    let task: &_ = Box::leak(Box::new(RefCell::new(None)));
    LocalSet::new()
        .run_until(async move {
            task::spawn_local(async move {
                let mut server = ServerParameters::default()
                    .bind(server_addr)
                    .await
                    .expect("server socket bind");

                loop {
                    let server::Connection {
                        sender: tx,
                        receiver: rx,
                        ..
                    } = server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(task::spawn_local(async move {
                        let mut tx = tx;
                        let mut rx = rx;
                        for i in 0 .. amount {
                            tx.send_reliable(format!("HelloWorld{}", i).as_bytes())
                                .await
                                .expect("server sent packet");
                        }

                        tx.wait_complete().await.expect("waiting for delivery");

                        for i in 0 .. amount {
                            let msg = rx.recv().await.expect("client message receive");
                            assert_eq!(msg.data().as_ref(), format!("HelloWorld{}", i).as_bytes());
                        }

                        task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });
                    }));
                }
            });

            time::sleep(Duration::from_millis(5)).await;

            let client = Client::bind(client_addr).await.expect("client bound");

            let client::Connection {
                sender: mut tx,
                receiver: mut rx,
                ..
            } = client.connect(proxy_addr).await.expect("client connection");

            for i in 0 .. amount {
                let msg = rx.recv().await.expect("client message receive");
                assert_eq!(msg.data(), format!("HelloWorld{}", i).as_bytes());
            }

            task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

            for i in 0 .. amount {
                tx.send_reliable(format!("HelloWorld{}", i).as_bytes())
                    .await
                    .expect("server sent packet");
            }

            tx.wait_complete().await.expect("waiting for delivery");

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;
}

#[test]
#[cfg(not(debug_assertions))]
fn reliable_test_load() {
    let _ = env_logger::try_init();

    use tokio::runtime::Builder as RTBuilder;

    let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

    let client_port = 30000 + test_num * 10 + 1;
    let server_port = 30000 + test_num * 10;

    std::thread::spawn(move || {
        let rt = RTBuilder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(LocalSet::new().run_until(async {
            let mut server = ServerParameters::default()
                .bind(([127, 0, 0, 1], server_port))
                .await
                .expect("server socket bind");

            let server::Connection {
                sender: mut tx,
                receiver: mut rx,
                ..
            } = server.accept().await.expect("connection accepted");

            let task = async move {
                for i in 0 .. 200000 {
                    tx.send_reliable(format!("HelloWorld{}", i).as_bytes())
                        .await
                        .expect("server sent packet");
                }

                for i in 0 .. 200000 {
                    let msg = rx.recv().await.expect("client message receive");
                    assert_eq!(msg.data().as_ref(), format!("HelloWorld{}", i).as_bytes());
                }
            };

            task::spawn_local(async move {
                loop {
                    let _ = server.accept().await.expect("connection accepted");
                }
            });

            task.await;
        }));
    });

    let rt = RTBuilder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(LocalSet::new().run_until(async {
        time::sleep(Duration::from_millis(5)).await;

        let client = Client::bind(([127, 0, 0, 1], client_port))
            .await
            .expect("client bound");

        let client::Connection {
            sender: mut tx,
            receiver: mut rx,
            ..
        } = client
            .connect(([127, 0, 0, 1], server_port))
            .await
            .expect("client connection");

        for i in 0 .. 200000 {
            let msg = rx.recv().await.expect("client message receive");
            assert_eq!(msg.data(), format!("HelloWorld{}", i).as_bytes());
        }

        task::spawn_local(async move {
            while let Ok(_) = rx.recv().await {}
            panic!("recv loop ended");
        });

        for i in 0 .. 200000 {
            tx.send_reliable(format!("HelloWorld{}", i).as_bytes())
                .await
                .expect("server sent packet");
        }
    }));
}
