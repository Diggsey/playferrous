use std::{
    fmt,
    future::Future,
    io, mem,
    pin::Pin,
    task::{ready, Context, Poll},
};

use futures::FutureExt;
use russh::{ChannelId, CryptoVec};
use tokio::io::AsyncWrite;
use tracing::instrument;

struct ChannelHandle {
    handle: russh::server::Handle,
    channel: ChannelId,
    closed: bool,
}

impl fmt::Debug for ChannelHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChannelHandle")
            .field("channel", &self.channel)
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}

impl ChannelHandle {
    #[instrument(level = "debug", fields(bytes=std::str::from_utf8(&*bytes).ok()))]
    async fn data(self, bytes: CryptoVec) -> (Self, Result<(), ()>) {
        let res = self.handle.data(self.channel, bytes).await.map_err(|_| ());
        (self, res)
    }
    #[instrument(level = "debug")]
    async fn shutdown(mut self) -> (Self, Result<(), ()>) {
        let res = self.handle.close(self.channel).await;
        self.closed = true;
        (self, res)
    }
}

enum DataWriterInner {
    Idle(ChannelHandle),
    Active(Pin<Box<dyn Future<Output = (ChannelHandle, Result<(), ()>)> + Send>>),
    Invalid,
}

impl fmt::Debug for DataWriterInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle(arg0) => f.debug_tuple("Idle").field(arg0).finish(),
            Self::Active(_) => f.debug_struct("Active").finish_non_exhaustive(),
            Self::Invalid => write!(f, "Invalid"),
        }
    }
}

impl DataWriterInner {
    pub fn new(handle: russh::server::Handle, channel: ChannelId) -> Self {
        Self::Idle(ChannelHandle {
            handle,
            channel,
            closed: false,
        })
    }

    fn poll_idle(&mut self, cx: &mut Context<'_>) -> Poll<(ChannelHandle, Result<(), io::Error>)> {
        match mem::replace(self, Self::Invalid) {
            Self::Idle(ch) => Poll::Ready((ch, Ok(()))),
            Self::Active(mut f) => match f.poll_unpin(cx) {
                Poll::Ready((ch, res)) => Poll::Ready((
                    ch,
                    res.map_err(|_| {
                        io::Error::new(io::ErrorKind::WriteZero, "Client disconnected")
                    }),
                )),
                Poll::Pending => {
                    *self = Self::Active(f);
                    Poll::Pending
                }
            },
            Self::Invalid => unreachable!(),
        }
    }
    fn poll_write(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut CryptoVec,
    ) -> Poll<Result<(), io::Error>> {
        let (ch, res) = ready!(self.poll_idle(cx));
        Poll::Ready(match res {
            Ok(()) => {
                *self = Self::Active(ch.data(mem::replace(buf, CryptoVec::new())).boxed());
                Ok(())
            }
            Err(e) => {
                *self = Self::Idle(ch);
                Err(e)
            }
        })
    }

    fn poll_flush(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let (ch, res) = ready!(self.poll_idle(cx));
        *self = Self::Idle(ch);
        Poll::Ready(res)
    }

    fn poll_shutdown(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        loop {
            let (ch, res) = ready!(self.poll_idle(cx));
            break Poll::Ready(match res {
                Ok(()) => {
                    if ch.closed {
                        *self = Self::Idle(ch);
                        Ok(())
                    } else {
                        *self = Self::Active(ch.shutdown().boxed());
                        continue;
                    }
                }
                Err(e) => {
                    *self = Self::Idle(ch);
                    Err(e)
                }
            });
        }
    }
}

#[derive(Debug)]
pub struct DataWriter {
    inner: DataWriterInner,
    buffer: CryptoVec,
}

impl DataWriter {
    pub fn new(handle: russh::server::Handle, channel: ChannelId) -> Self {
        Self {
            inner: DataWriterInner::new(handle, channel),
            buffer: CryptoVec::new(),
        }
    }
}

impl AsyncWrite for DataWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let this = &mut *self;
        if this.buffer.len() >= 1024 {
            ready!(this.inner.poll_write(cx, &mut this.buffer))?;
        }
        self.buffer.extend(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let this = &mut *self;
        if !this.buffer.is_empty() {
            ready!(this.inner.poll_write(cx, &mut this.buffer))?;
        }
        this.inner.poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        let this = &mut *self;
        if !this.buffer.is_empty() {
            ready!(this.inner.poll_write(cx, &mut this.buffer))?;
        }
        this.inner.poll_shutdown(cx)
    }
}
