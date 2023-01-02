use crate::SendError;
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{
        Context,
        Poll,
        Waker,
    },
};

struct Shared<T> {
    value: Option<T>,
    waker: Option<Waker>,
    is_open: bool,
}

pub struct Sender<T> {
    shared: Rc<RefCell<Shared<T>>>,
}

impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut shared = self.shared.borrow_mut();
        if shared.is_open {
            shared.value = Some(value);
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
        if shared.is_open {
            if let Some(waker) = shared.waker.take() {
                waker.wake();
            }
        }
        shared.is_open = false;
    }
}

pub struct Receiver<T> {
    shared: Rc<RefCell<Shared<T>>>,
}

impl<T> Future for Receiver<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut shared = self.shared.borrow_mut();
        match shared.value.take() {
            Some(value) => Poll::Ready(Some(value)),
            None => {
                if shared.is_open {
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
        shared.is_open = false;
    }
}

pub fn oneshot<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Rc::new(RefCell::new(Shared {
        value: None,
        waker: None,
        is_open: true,
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
    use futures_lite::future;

    #[test]
    fn test() {
        future::block_on(async {
            let (tx, rx) = oneshot();
            tx.send("test").unwrap();
            drop(rx);
            assert!(tx.send("test").is_err());

            let (tx, rx) = oneshot();
            tx.send("test").unwrap();
            assert_eq!(rx.await.unwrap(), "test");
            drop(tx);

            let (tx, rx) = oneshot();
            tx.send("test").unwrap();
            drop(tx);
            assert_eq!(rx.await.unwrap(), "test");
        });
    }
}
