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
    is_closed: bool,
}

pub struct Sender<T> {
    shared: Rc<RefCell<Shared<T>>>,
}

impl<T> Sender<T> {
    pub fn send(&self, value: T) {
        let mut shared = self.shared.borrow_mut();
        shared.value = Some(value);
        if let Some(waker) = shared.waker.take() {
            waker.wake();
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut shared = self.shared.borrow_mut();
        shared.is_closed = true;
        if let Some(waker) = shared.waker.take() {
            waker.wake();
        }
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
                if shared.is_closed {
                    Poll::Ready(None)
                } else {
                    shared.waker = Some(cx.waker().clone());
                    Poll::Pending
                }
            },
        }
    }
}

pub fn oneshot<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Rc::new(RefCell::new(Shared {
        value: None,
        waker: None,
        is_closed: false,
    }));

    (
        Sender {
            shared: shared.clone(),
        },
        Receiver { shared },
    )
}
