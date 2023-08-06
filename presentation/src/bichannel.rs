use tokio::sync::mpsc;

#[derive(Debug)]
pub struct Bichannel<S, R = S> {
    pub s: mpsc::Sender<S>,
    pub r: mpsc::Receiver<R>,
}

pub fn bichannel<S, R>(buffer: usize) -> (Bichannel<S, R>, Bichannel<R, S>) {
    let (s1, r1) = mpsc::channel(buffer);
    let (s2, r2) = mpsc::channel(buffer);
    (Bichannel { s: s1, r: r2 }, Bichannel { s: s2, r: r1 })
}
