//! A oneshot [`!Send`] and [`!Sync`] async channel.
//!
//! There are [`Sender`] and [`Receiver`] sides.
//!
//! When either the [`Sender`] or the [`Receiver`] is dropped, the channel becomes closed. When a
//! channel is closed, no more messages can be sent, but remaining messages can still be received.
//! The same [`Sender`] and [`Receiver`] pair can be reused, but only will keep the latest sent
//! value inside.
//!
//! # Examples
//!
//! ```
//! futures_lite::future::block_on(async {
//!     let (tx, mut rx) = local_channel::oneshot::oneshot();
//!
//!     assert!(tx.send("HelloWorld!").is_ok());
//!     assert_eq!((&mut rx).await, Ok("HelloWorld!"));
//!     assert!(tx.send("HelloWorldAgain!").is_ok());
//!     assert_eq!(rx.await, Ok("HelloWorldAgain!"));
//! });
//! ```

use crate::{
    ReceiveError,
    SendError,
    TryReceiveError,
};
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

/// Sends values to the associated `Receiver`.
pub struct Sender<T> {
    shared: Rc<RefCell<Shared<T>>>,
}

impl<T> Sender<T> {
    /// Sends a value to be received by the `Receiver`.
    /// Returns either `Ok`, if the value sent successfully, or
    /// `Err` with the sent value, if the channel is closed.
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

/// Receives values from the associated `Sender`.
/// Implements `Future` and can either consumed directly
/// or by mutable reference.
pub struct Receiver<T> {
    shared: Rc<RefCell<Shared<T>>>,
}

impl<T> Receiver<T> {
    /// Tries to get an already sent value.
    pub fn try_recv(&mut self) -> Result<T, TryReceiveError> {
        let mut shared = self.shared.borrow_mut();

        if !shared.is_open {
            return Err(TryReceiveError::Closed);
        }

        shared.value.take().ok_or(TryReceiveError::Empty)
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, ReceiveError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut shared = self.shared.borrow_mut();
        match shared.value.take() {
            Some(value) => Poll::Ready(Ok(value)),
            None => {
                if shared.is_open {
                    shared.waker = Some(cx.waker().clone());
                    Poll::Pending
                } else {
                    Poll::Ready(Err(ReceiveError::Closed))
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

/// Creates a oneshot channel and returns both `Sender` and `Receiver` sides as a tuple.
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

            let (tx, mut rx) = oneshot();
            tx.send("test").unwrap();
            assert_eq!((&mut rx).await.unwrap(), "test");
            tx.send("test1").unwrap();
            assert_eq!(rx.await.unwrap(), "test1");
        });
    }
}
