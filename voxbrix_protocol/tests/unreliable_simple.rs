use futures_lite::{
    future,
    FutureExt,
};
use std::{
    future::Future,
    iter,
    sync::atomic::{
        AtomicU16,
        Ordering,
    },
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
};

static TEST_NUM_DISPENCER: AtomicU16 = AtomicU16::new(0);

async fn client_server_test<'s, 'c>(
    mut server_check: impl AsyncFnMut(&mut server::Connection) + 's,
    mut client_check: impl AsyncFnMut(&mut client::Connection) + 'c,
) -> (impl Future<Output = ()> + 's, impl Future<Output = ()> + 'c) {
    let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

    let client_port = 30000 + test_num * 10 + 1;
    let server_port = 30000 + test_num * 10;

    let client_addr = ([127, 0, 0, 1], client_port);
    let server_addr = ([127, 0, 0, 1], server_port);

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

        let mut conn = client
            .connect(server_addr)
            .await
            .expect("client connection");

        client_check(&mut conn).await;
    };

    (server_task, client_task)
}

#[tokio::test]
async fn unreliable_test_0() {
    let _ = env_logger::try_init();

    let server_check = async |conn: &mut server::Connection| {
        conn.sender
            .send_unreliable(b"1HelloWorld1")
            .await
            .expect("server sent packet");

        let msg = conn.receiver.recv().await.unwrap();

        assert_eq!(msg.data().as_ref(), b"2HelloWorld2");
    };

    let client_check = async |conn: &mut client::Connection| {
        let msg = conn.receiver.recv().await.expect("client message receive");

        assert_eq!(msg.data().as_ref(), b"1HelloWorld1");

        conn.sender
            .send_unreliable(b"2HelloWorld2")
            .await
            .expect("client sent packet");
    };

    let (server_task, client_task) = client_server_test(server_check, client_check).await;

    future::zip(server_task, client_task).await;
}

#[tokio::test]
async fn unreliable_test_1() {
    let _ = env_logger::try_init();

    let data_slice = &[1, 2, 3, 4, 5];
    let data = iter::repeat(data_slice)
        .take(300)
        .flatten()
        .cloned()
        .collect::<Vec<_>>();

    let server_check = async |conn: &mut server::Connection| {
        conn.sender
            .send_unreliable(&data)
            .await
            .expect("server sent packet");
    };

    let client_check = async |conn: &mut client::Connection| {
        let msg = conn.receiver.recv().await.expect("client message receive");

        assert_eq!(msg.data().as_ref(), data.as_slice());
    };

    let (server_task, client_task) = client_server_test(server_check, client_check).await;

    future::zip(server_task, client_task).await;
}

#[tokio::test]
async fn unreliable_test_2() {
    let _ = env_logger::try_init();

    let data_slice = &[1, 2, 3, 4, 5];
    let data = iter::repeat(data_slice)
        .take(300)
        .flatten()
        .cloned()
        .collect::<Vec<_>>();

    let server_check = async |conn: &mut server::Connection| {
        let msg = conn.receiver.recv().await.expect("server message receive");

        assert_eq!(msg.data().as_ref(), data.as_slice());
    };

    let client_check = async |conn: &mut client::Connection| {
        conn.sender
            .send_unreliable(&data)
            .await
            .expect("client sent packet");
    };

    let (server_task, client_task) = client_server_test(server_check, client_check).await;

    future::zip(server_task, client_task).await;
}

#[tokio::test]
async fn unreliable_test_3() {
    let _ = env_logger::try_init();

    let data_slice = &[1, 2, 3, 4, 5];
    let data = iter::repeat(data_slice)
        .take(300)
        .flatten()
        .cloned()
        .collect::<Vec<_>>();

    let server_check = async |conn: &mut server::Connection| {
        for i in 0 .. 10 {
            let msg = conn.receiver.recv().await.expect("server received data");

            assert_eq!(msg.data().as_ref(), &[data.as_slice(), &[i]].concat());
        }
    };

    let client_check = async |conn: &mut client::Connection| {
        for i in 0 .. 10 {
            conn.sender
                .send_unreliable(&[data.as_slice(), &[i]].concat())
                .await
                .expect("client sent data");
        }
    };

    let (server_task, client_task) = client_server_test(server_check, client_check).await;

    future::zip(server_task, client_task).await;
}

#[tokio::test]
async fn unreliable_test_4() {
    let _ = env_logger::try_init();

    let data_slice = &[1, 2, 3, 4, 5];
    let data = iter::repeat(data_slice)
        .take(300)
        .flatten()
        .cloned()
        .collect::<Vec<_>>();

    let server_check = async |conn: &mut server::Connection| {
        for i in 20 .. 30 {
            conn.sender
                .send_unreliable(&[data.as_slice(), &[i]].concat())
                .await
                .expect("server sent data");
        }
        for i in 50 .. 60 {
            conn.sender
                .send_unreliable(&[data.as_slice(), &[i]].concat())
                .await
                .expect("server sent data");
        }
        for i in 0 .. 10 {
            let msg = conn.receiver.recv().await.expect("server received data");

            assert_eq!(msg.data().as_ref(), &[data.as_slice(), &[i]].concat());
        }
        for i in 90 .. 100 {
            let msg = conn.receiver.recv().await.expect("server received data");

            assert_eq!(msg.data().as_ref(), &[data.as_slice(), &[i]].concat());
        }
    };

    let client_check = async |conn: &mut client::Connection| {
        for i in 20 .. 30 {
            let msg = conn.receiver.recv().await.expect("client received data");

            assert_eq!(msg.data().as_ref(), &[data.as_slice(), &[i]].concat());
        }

        for i in 50 .. 60 {
            let msg = conn.receiver.recv().await.expect("client received data");

            assert_eq!(msg.data().as_ref(), &[data.as_slice(), &[i]].concat());
        }

        for i in 0 .. 10 {
            conn.sender
                .send_unreliable(&[data.as_slice(), &[i]].concat())
                .await
                .expect("client sent data");
        }

        for i in 90 .. 100 {
            conn.sender
                .send_unreliable(&[data.as_slice(), &[i]].concat())
                .await
                .expect("client sent data");
        }
    };

    let (server_task, client_task) = client_server_test(server_check, client_check).await;

    future::zip(server_task, client_task).await;
}
