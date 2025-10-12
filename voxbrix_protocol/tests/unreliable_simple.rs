use std::{
    cell::RefCell,
    iter,
    sync::atomic::{
        AtomicU16,
        Ordering,
    },
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
};

static TEST_NUM_DISPENCER: AtomicU16 = AtomicU16::new(0);

#[tokio::test]
async fn unreliable_test_0() {
    let _ = env_logger::try_init();

    let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

    let client_port = 30000 + test_num * 10 + 1;
    let server_port = 30000 + test_num * 10;

    let task: &_ = Box::leak(Box::new(RefCell::new(None)));

    LocalSet::new()
        .run_until(async move {
            task::spawn_local(async move {
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .await
                    .expect("server socket bind");
                loop {
                    let server::Connection {
                        sender: mut tx,
                        receiver: mut rx,
                        ..
                    } = server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(task::spawn_local(async move {
                        tx.send_unreliable(b"1HelloWorld1")
                            .await
                            .expect("server sent packet");

                        let msg = rx.recv().await.unwrap();

                        assert_eq!(msg.data().as_ref(), b"2HelloWorld2");
                    }));
                }
            });

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

            let msg = rx.recv().await.expect("client message receive");

            assert_eq!(msg.data().as_ref(), b"1HelloWorld1");

            tx.send_unreliable(b"2HelloWorld2")
                .await
                .expect("client sent packet");

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;
}

#[tokio::test]
async fn unreliable_test_1() {
    let _ = env_logger::try_init();

    let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

    let client_port = 30000 + test_num * 10 + 1;
    let server_port = 30000 + test_num * 10;

    let data: &_ = Box::leak(Box::new({
        let data_slice = &[1, 2, 3, 4, 5];
        iter::repeat(data_slice)
            .take(300)
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
    }));

    LocalSet::new()
        .run_until(async move {
            task::spawn_local(async move {
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .await
                    .expect("server socket bind");
                loop {
                    let server::Connection { sender: mut tx, .. } =
                        server.accept().await.expect("connection accepted");

                    task::spawn_local(async move {
                        tx.send_unreliable(data).await.expect("server sent packet");
                    });
                }
            });

            time::sleep(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port))
                .await
                .expect("client bound");

            let client::Connection {
                receiver: mut rx, ..
            } = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            let msg = rx.recv().await.expect("client message receive");

            assert_eq!(msg.data().as_ref(), data.as_slice());
        })
        .await;
}

#[tokio::test]
async fn unreliable_test_2() {
    let _ = env_logger::try_init();

    let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

    let client_port = 30000 + test_num * 10 + 1;
    let server_port = 30000 + test_num * 10;

    let task: &_ = Box::leak(Box::new(RefCell::new(None)));
    let data: &_ = Box::leak(Box::new({
        let data_slice = &[1, 2, 3, 4, 5];
        iter::repeat(data_slice)
            .take(300)
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
    }));

    LocalSet::new()
        .run_until(async move {
            task::spawn_local(async move {
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .await
                    .expect("server socket bind");
                loop {
                    let server::Connection {
                        receiver: mut rx, ..
                    } = server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(task::spawn_local(async move {
                        let msg = rx.recv().await.expect("server received data");

                        assert_eq!(msg.data().as_ref(), data.as_slice());
                    }));
                }
            });

            time::sleep(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port))
                .await
                .expect("client bound");

            let client::Connection { sender: mut tx, .. } = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            tx.send_unreliable(&data).await.expect("client sent data");

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;
}

#[tokio::test]
async fn unreliable_test_3() {
    let _ = env_logger::try_init();

    let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

    let client_port = 30000 + test_num * 10 + 1;
    let server_port = 30000 + test_num * 10;

    let task: &_ = Box::leak(Box::new(RefCell::new(None)));
    let data: &_ = Box::leak(Box::new({
        let data_slice = &[1, 2, 3, 4, 5];
        iter::repeat(data_slice)
            .take(300)
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
    }));

    LocalSet::new()
        .run_until(async move {
            task::spawn_local(async move {
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .await
                    .expect("server socket bind");
                loop {
                    let server::Connection {
                        receiver: mut rx, ..
                    } = server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(task::spawn_local(async move {
                        for i in 0 .. 10 {
                            let msg = rx.recv().await.expect("server received data");

                            assert_eq!(
                                msg.data().as_ref(),
                                [data.as_slice(), &[i]]
                                    .into_iter()
                                    .flatten()
                                    .map(|i| *i)
                                    .collect::<Vec<u8>>()
                                    .as_slice()
                            );
                        }
                    }));
                }
            });

            time::sleep(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port))
                .await
                .expect("client bound");

            let client::Connection { sender: mut tx, .. } = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            for i in 0 .. 10 {
                tx.send_unreliable(
                    [data.as_slice(), &[i]]
                        .into_iter()
                        .flatten()
                        .map(|i| *i)
                        .collect::<Vec<u8>>()
                        .as_slice(),
                )
                .await
                .expect("client sent data");
            }

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;
}

#[tokio::test]
async fn unreliable_test_4() {
    let _ = env_logger::try_init();

    let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

    let client_port = 30000 + test_num * 10 + 1;
    let server_port = 30000 + test_num * 10;

    let task: &_ = Box::leak(Box::new(RefCell::new(None)));
    let data: &_ = Box::leak(Box::new({
        let data_slice = &[1, 2, 3, 4, 5];
        iter::repeat(data_slice)
            .take(300)
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
    }));

    LocalSet::new()
        .run_until(async move {
            task::spawn_local(async move {
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .await
                    .expect("server socket bind");
                loop {
                    let server::Connection {
                        sender: mut tx,
                        receiver: mut rx,
                        ..
                    } = server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(task::spawn_local(async move {
                        for i in 20 .. 30 {
                            tx.send_unreliable(
                                [data.as_slice(), &[i]]
                                    .into_iter()
                                    .flatten()
                                    .map(|i| *i)
                                    .collect::<Vec<u8>>()
                                    .as_slice(),
                            )
                            .await
                            .expect("server sent data");
                        }
                        for i in 50 .. 60 {
                            tx.send_unreliable(
                                [data.as_slice(), &[i]]
                                    .into_iter()
                                    .flatten()
                                    .map(|i| *i)
                                    .collect::<Vec<u8>>()
                                    .as_slice(),
                            )
                            .await
                            .expect("server sent data");
                        }
                        for i in 0 .. 10 {
                            let msg = rx.recv().await.expect("server received data");

                            assert_eq!(
                                msg.data().as_ref(),
                                [data.as_slice(), &[i]]
                                    .into_iter()
                                    .flatten()
                                    .map(|i| *i)
                                    .collect::<Vec<u8>>()
                                    .as_slice()
                            );
                        }
                        for i in 90 .. 100 {
                            let msg = rx.recv().await.expect("server received data");

                            assert_eq!(
                                msg.data().as_ref(),
                                [data.as_slice(), &[i]]
                                    .into_iter()
                                    .flatten()
                                    .map(|i| *i)
                                    .collect::<Vec<u8>>()
                                    .as_slice()
                            );
                        }
                    }));
                }
            });

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

            for i in 20 .. 30 {
                let msg = rx.recv().await.expect("client received data");

                assert_eq!(
                    msg.data().as_ref(),
                    [data.as_slice(), &[i]]
                        .into_iter()
                        .flatten()
                        .map(|i| *i)
                        .collect::<Vec<u8>>()
                        .as_slice()
                );
            }

            for i in 50 .. 60 {
                let msg = rx.recv().await.expect("client received data");

                assert_eq!(
                    msg.data().as_ref(),
                    [data.as_slice(), &[i]]
                        .into_iter()
                        .flatten()
                        .map(|i| *i)
                        .collect::<Vec<u8>>()
                        .as_slice()
                );
            }

            for i in 0 .. 10 {
                tx.send_unreliable(
                    [data.as_slice(), &[i]]
                        .into_iter()
                        .flatten()
                        .map(|i| *i)
                        .collect::<Vec<u8>>()
                        .as_slice(),
                )
                .await
                .expect("client sent data");
            }

            for i in 90 .. 100 {
                tx.send_unreliable(
                    [data.as_slice(), &[i]]
                        .into_iter()
                        .flatten()
                        .map(|i| *i)
                        .collect::<Vec<u8>>()
                        .as_slice(),
                )
                .await
                .expect("client sent data");
            }

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;
}
