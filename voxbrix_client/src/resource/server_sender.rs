use flume::Sender;

pub struct ServerSender {
    pub unreliable: Sender<Vec<u8>>,
}
