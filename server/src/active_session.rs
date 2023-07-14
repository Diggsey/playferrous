use tokio::sync::mpsc;

pub enum ClientSessionCommand {
    TerminalLine(String),
}

#[derive(Debug, Clone)]
pub enum ServerSessionCommand {
    TerminalPrint(String),
    TerminalRequestLine,
}

#[derive(Debug)]
pub struct ActiveSession {
    pub tx: mpsc::Sender<ClientSessionCommand>,
    pub rx: mpsc::Receiver<ServerSessionCommand>,
}

#[derive(Debug)]
pub struct SessionLink {
    pub tx: mpsc::Sender<ServerSessionCommand>,
    pub rx: mpsc::Receiver<ClientSessionCommand>,
}
