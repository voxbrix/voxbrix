use crate::SendError;
use futures_core::Stream;
use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{
        Context,
        Poll,
        Waker,
    },
};

#[derive(Debug)]
struct Shared<T> {
    queue: VecDeque<T>,
    waker: Option<Waker>,
    has_receiver: bool,
}

#[derive(Debug)]
pub struct Sender<T> {
    shared: Rc<RefCell<Shared<T>>>,
}

impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut shared = self.shared.borrow_mut();
        if shared.has_receiver {
            shared.queue.push_back(value);
            if let Some(waker) = shared.waker.take() {
                waker.wake();
            }
            Ok(())
        } else {
            Err(SendError(value))
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut shared = self.shared.borrow_mut();
        if Rc::strong_count(&self.shared) == 2 && shared.has_receiver {
            if let Some(waker) = shared.waker.take() {
                waker.wake();
            }
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Receiver<T> {
    shared: Rc<RefCell<Shared<T>>>,
}

impl<T> Receiver<T> {
    pub fn recv(&mut self) -> Receive<'_, T> {
        Receive { receiver: self }
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut shared = self.shared.borrow_mut();
        match shared.queue.pop_front() {
            Some(value) => Poll::Ready(Some(value)),
            None => {
                if Rc::strong_count(&self.shared) > 1 {
                    shared.waker = Some(cx.waker().clone());
                    Poll::Pending
                } else {
                    Poll::Ready(None)
                }
            },
        }
    }
}

#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Receive<'a, T> {
    receiver: &'a mut Receiver<T>,
}

impl<'a, T> Future for Receive<'a, T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut shared = self.receiver.shared.borrow_mut();
        match shared.queue.pop_front() {
            Some(value) => Poll::Ready(Some(value)),
            None => {
                if Rc::strong_count(&self.receiver.shared) > 1 {
                    shared.waker = Some(cx.waker().clone());
                    Poll::Pending
                } else {
                    Poll::Ready(None)
                }
            },
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut shared = self.shared.borrow_mut();
        shared.has_receiver = false;
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Rc::new(RefCell::new(Shared {
        queue: VecDeque::new(),
        waker: None,
        has_receiver: true,
    }));

    (
        Sender {
            shared: shared.clone(),
        },
        Receiver { shared },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_lite::{
        future,
        stream::StreamExt as _,
    };

    #[test]
    fn test() {
        future::block_on(async {
            let (tx, mut rx) = channel();
            tx.send("test").unwrap();
            assert_eq!(future::poll_once(rx.recv()).await, Some(Some("test")),);

            let tx2 = tx.clone();
            tx2.send("test2").unwrap();
            assert_eq!(future::poll_once(rx.recv()).await, Some(Some("test2")),);
            assert_eq!(future::poll_once(rx.recv()).await, None);
            drop(tx2);

            tx.send("test").unwrap();
            tx.send("test2").unwrap();
            assert_eq!(future::poll_once(rx.recv()).await, Some(Some("test")),);
            assert_eq!(future::poll_once(rx.recv()).await, Some(Some("test2")),);
            drop(tx);
            assert_eq!(rx.next().await, None);

            let (tx, rx) = channel();
            tx.send("test").unwrap();
            drop(rx);
            assert!(tx.send("test").is_err());

            let (tx, mut rx) = channel();
            tx.send("test").unwrap();
            assert_eq!(rx.recv().await.unwrap(), "test");
            drop(tx);

            let (tx, mut rx) = channel();
            tx.send("test").unwrap();
            assert_eq!(rx.recv().await.unwrap(), "test");
            drop(tx);
            assert!(rx.recv().await.is_none());
        });
    }
}
