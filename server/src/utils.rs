use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::{future::FusedFuture, Future};
use pin_project_lite::pin_project;
use rand::{seq::SliceRandom, thread_rng};

#[derive(Debug)]
pub struct Select<T> {
    futures: Pin<Box<[T]>>,
}

fn iter_pin_mut<T>(slice: Pin<&mut [T]>) -> impl Iterator<Item = Pin<&mut T>> {
    // Borrowed from futures-util
    // Safety: `std` _could_ make this unsound if it were to decide Pin's
    // invariants aren't required to transmit through slices. Otherwise this has
    // the same safety as a normal field pin projection.
    unsafe { slice.get_unchecked_mut() }
        .iter_mut()
        .map(|t| unsafe { Pin::new_unchecked(t) })
}

impl<T: Future> Future for Select<T> {
    type Output = T::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let res = iter_pin_mut(self.futures.as_mut()).find_map(|fut| match fut.poll(cx) {
            Poll::Ready(x) => Some(x),
            Poll::Pending => None,
        });
        match res {
            Some(x) => {
                self.futures = Box::pin([]);
                Poll::Ready(x)
            }
            None => Poll::Pending,
        }
    }
}

impl<T: Future> FusedFuture for Select<T> {
    fn is_terminated(&self) -> bool {
        self.futures.is_empty()
    }
}

pub trait FutureIteratorExt: IntoIterator {
    fn select(self) -> Select<Self::Item>;
}

impl<I: IntoIterator> FutureIteratorExt for I
where
    I::Item: Future,
{
    fn select(self) -> Select<Self::Item> {
        let mut futures: Box<_> = self.into_iter().collect();
        if futures.len() > 1 {
            // Implement randomness
            futures.shuffle(&mut thread_rng());
        }
        Select {
            futures: Box::into_pin(futures),
        }
    }
}

pub trait FutureExt2: Future {
    fn with_key<K>(self, key: K) -> WithKey<Self, K>
    where
        Self: Sized;
}

pin_project! {
    #[derive(Debug)]
    pub struct WithKey<T, K> {
        key: Option<K>,
        #[pin]
        fut: T,
    }
}

impl<T: Future, K> Future for WithKey<T, K> {
    type Output = (K, T::Output);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if this.key.is_some() {
            this.fut.poll(cx).map(|x| (this.key.take().unwrap(), x))
        } else {
            Poll::Pending
        }
    }
}

impl<T: Future, K> FusedFuture for WithKey<T, K> {
    fn is_terminated(&self) -> bool {
        self.key.is_none()
    }
}

impl<T: Future> FutureExt2 for T {
    fn with_key<K>(self, key: K) -> WithKey<Self, K>
    where
        Self: Sized,
    {
        WithKey {
            key: Some(key),
            fut: self,
        }
    }
}
