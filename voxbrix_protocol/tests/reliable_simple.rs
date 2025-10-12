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

static TEST_NUM_DISPENCER: AtomicU16 = AtomicU16::new(1000);

#[tokio::test]
async fn reliable_test_0() {
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

                    task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

                    *task.borrow_mut() = Some(task::spawn_local(async move {
                        tx.send_reliable(b"HelloWorld")
                            .await
                            .expect("server sent packet");
                    }));
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

            assert_eq!(msg.data(), b"HelloWorld");

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;
}

#[tokio::test]
async fn reliable_test_1() {
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
                    let server::Connection { sender: mut tx, .. } =
                        server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(task::spawn_local(async move {
                        tx.send_reliable(b"HelloWorld")
                            .await
                            .expect("server sent packet");
                    }));
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

            assert_eq!(msg.data(), b"HelloWorld");

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;
}

#[tokio::test]
async fn reliable_test_2() {
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
                    let server::Connection { sender: mut tx, .. } =
                        server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(task::spawn_local(async move {
                        for i in 0 .. 1000 {
                            tx.send_reliable(format!("HelloWorld{}", i).as_bytes())
                                .await
                                .expect("server sent packet");
                        }
                    }));
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

            for i in 0 .. 1000 {
                let msg = rx.recv().await.expect("client message receive");
                assert_eq!(msg.data(), format!("HelloWorld{}", i).as_bytes());
            }

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;
}

#[tokio::test]
async fn reliable_test_3() {
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
                        for i in 0 .. 1000 {
                            tx.send_reliable(format!("HelloWorld{}", i).as_bytes())
                                .await
                                .expect("server sent packet");
                        }

                        for i in 0 .. 1000 {
                            let msg = rx.recv().await.expect("client message receive");
                            assert_eq!(msg.data().as_ref(), format!("HelloWorld{}", i).as_bytes());
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

            for i in 0 .. 1000 {
                let msg = rx.recv().await.expect("client message receive");
                assert_eq!(msg.data(), format!("HelloWorld{}", i).as_bytes());
            }

            task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

            for i in 0 .. 1000 {
                tx.send_reliable(format!("HelloWorld{}", i).as_bytes())
                    .await
                    .expect("server sent packet");
            }

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;
}

#[tokio::test]
async fn reliable_test_4() {
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
                        tx.send_reliable(data.as_ref())
                            .await
                            .expect("server sent packet");

                        let msg = rx.recv().await.expect("client message receive");
                        assert_eq!(msg.data().as_ref(), data.as_slice());
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
            assert_eq!(msg.data().as_ref(), data.as_slice());

            task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

            tx.send_reliable(data.as_ref())
                .await
                .expect("server sent packet");

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;
}

#[tokio::test]
async fn reliable_test_5() {
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
                        for i in 0 .. 10 {
                            let data: &_ = Box::leak(Box::new({
                                let data_slice = &[i + 1, i + 2, i + 3, i + 4, i + 5];
                                iter::repeat(data_slice)
                                    .take(300)
                                    .flatten()
                                    .cloned()
                                    .collect::<Vec<_>>()
                            }));

                            tx.send_reliable(data.as_ref())
                                .await
                                .expect("server sent packet");
                        }

                        for i in 0 .. 10 {
                            let msg = rx.recv().await.expect("client message receive");

                            let data: &_ = Box::leak(Box::new({
                                let data_slice = &[i + 1, i + 2, i + 3, i + 4, i + 5];
                                iter::repeat(data_slice)
                                    .take(300)
                                    .flatten()
                                    .cloned()
                                    .collect::<Vec<_>>()
                            }));

                            assert_eq!(msg.data().as_ref(), data.as_slice());
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

            for i in 0 .. 10 {
                let msg = rx.recv().await.expect("client message receive");

                let data: &_ = Box::leak(Box::new({
                    let data_slice = &[i + 1, i + 2, i + 3, i + 4, i + 5];
                    iter::repeat(data_slice)
                        .take(300)
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>()
                }));

                assert_eq!(msg.data().as_ref(), data.as_slice());
            }

            task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

            for i in 0 .. 10 {
                let data: &_ = Box::leak(Box::new({
                    let data_slice = &[i + 1, i + 2, i + 3, i + 4, i + 5];
                    iter::repeat(data_slice)
                        .take(300)
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>()
                }));
                tx.send_reliable(data.as_ref())
                    .await
                    .expect("server sent packet");
            }

            task.borrow_mut().take().unwrap().await.unwrap();
        })
        .await;
}
