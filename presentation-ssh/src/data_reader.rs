use std::{
    io,
    pin::Pin,
    task::{ready, Context, Poll},
};

use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::mpsc,
};
use tracing::debug;

#[derive(Debug)]
pub struct DataReader {
    receiver: mpsc::Receiver<Vec<u8>>,
    buffer: Vec<u8>,
    offset: usize,
}

impl DataReader {
    pub fn new(receiver: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            receiver,
            buffer: Vec::new(),
            offset: 0,
        }
    }
}

impl AsyncRead for DataReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.offset == self.buffer.len() {
            if let Some(buffer) = ready!(self.receiver.poll_recv(cx)) {
                debug!(
                    "DataReader::poll_read received {:?}",
                    std::str::from_utf8(&buffer).ok()
                );
                self.buffer = buffer;
                self.offset = 0;
            }
        }
        let amt = buf.remaining().min(self.buffer.len() - self.offset);
        buf.put_slice(&self.buffer[self.offset..(self.offset + amt)]);
        self.offset += amt;
        Poll::Ready(Ok(()))
    }
}
