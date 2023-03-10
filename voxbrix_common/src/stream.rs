use futures_core::Stream;
use pin_project_lite::pin_project;
use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

pub trait StreamExt {
    /// Fail-fast version of .or()
    fn or_ff<S, T>(self, other: S) -> OrFf<Self, S>
    where
        Self: Stream<Item = T> + Sized,
        S: Stream<Item = T>;
}

impl<S1> StreamExt for S1 {
    fn or_ff<S2, T>(self, other: S2) -> OrFf<Self, S2>
    where
        Self: Stream<Item = T>,
        S2: Stream<Item = T>,
    {
        OrFf {
            stream1: self,
            stream2: other,
        }
    }
}

pin_project! {
    #[must_use = "streams do nothing unless polled"]
    pub struct OrFf<S1, S2> {
        #[pin]
        stream1: S1,
        #[pin]
        stream2: S2,
    }
}

impl<S1, S2, T> Stream for OrFf<S1, S2>
where
    S1: Stream<Item = T>,
    S2: Stream<Item = T>,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        if let Poll::Ready(opt) = this.stream1.as_mut().poll_next(cx) {
            Poll::Ready(opt)
        } else {
            this.stream2.as_mut().poll_next(cx)
        }
    }
}
