use std::sync::Arc;

use aerosol::Aerosol;
use anyhow::Context;
use playferrous_presentation::{
    InvalidIdError, SessionId, TerminalClientCommand, TerminalConnection, TerminalServerCommand,
    UserId,
};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::error;

use crate::{
    active_session::{ActiveSession, ClientSessionCommand, ServerSessionCommand, SessionLink},
    database::{self, session::SessionType, TransactError},
    game_manager::GameManager,
    proposal_manager::ProposalManager,
};

use self::ui::{CommandInterpretation, Ui};

mod ui;

pub struct TerminalSession {
    aero: Aerosol,
    user_id: UserId,
    tx: mpsc::Sender<TerminalServerCommand>,
    rx: mpsc::Receiver<TerminalClientCommand>,
    active_session: Option<ActiveSession>,
}

enum TerminalSessionEvent {
    ClientCommand(Option<TerminalClientCommand>),
    SessionCommand(Option<ServerSessionCommand>),
}

#[derive(Debug, Error)]
enum TerminalError {
    #[error("{0}")]
    Prompt(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<TerminalError> for TransactError<TerminalError> {
    fn from(value: TerminalError) -> Self {
        Self::App(value)
    }
}

impl From<InvalidIdError> for TerminalError {
    fn from(_value: InvalidIdError) -> Self {
        Self::Prompt("Invalid ID\n".into())
    }
}

impl TerminalSession {
    pub fn spawn(aero: Aerosol, user_id: UserId) -> TerminalConnection {
        let (tx1, rx1) = mpsc::channel(4);
        let (tx2, rx2) = mpsc::channel(4);
        tokio::spawn(
            Self {
                aero,
                user_id,
                tx: tx2,
                rx: rx1,
                active_session: None,
            }
            .run(),
        );
        TerminalConnection {
            sender: tx1,
            receiver: rx2,
        }
    }
    async fn run(mut self) {
        if let Err(e) = self.run_inner().await {
            error!("TerminalSession: {}", e);
        }
    }
    async fn print(&mut self, text: String) -> anyhow::Result<()> {
        Ok(self.tx.send(TerminalServerCommand::Print { text }).await?)
    }
    fn unpack_args<const N: usize>(args: Vec<String>) -> Result<[String; N], TerminalError> {
        args.try_into().map_err(|args: Vec<_>| {
            TerminalError::Prompt(format!(
                "Expected {} arguments, received {}!\n",
                N,
                args.len()
            ))
        })
    }
    fn new_session_link(&mut self) -> SessionLink {
        let (tx0, rx0) = mpsc::channel(1);
        let (tx1, rx1) = mpsc::channel(1);
        self.active_session = Some(ActiveSession { tx: tx0, rx: rx1 });
        SessionLink { tx: tx1, rx: rx0 }
    }
    async fn propose(&mut self, [game_type]: [String; 1]) -> Result<(), TerminalError> {
        transact!(TerminalError, self.aero, |tx| {
            database::proposal::create(tx, &game_type, self.user_id).await?;
            Ok(())
        })
    }
    async fn sessions(&mut self) -> Result<(), TerminalError> {
        let sessions = transact!(TerminalError, self.aero, |tx| {
            Ok(database::session::list_for_user(tx, self.user_id).await?)
        })?;
        for session in sessions {
            self.print(format!(
                "{:>6} {:12?} {}\n",
                session.id, session.type_, session.created_at
            ))
            .await?;
        }
        Ok(())
    }
    async fn enter(&mut self, [session_id]: [String; 1]) -> Result<(), TerminalError> {
        let session_id = session_id.parse()?;

        let session = transact!(TerminalError, self.aero, |tx| {
            Ok(database::session::get_by_id(tx, session_id)
                .await?
                .ok_or_else(|| TerminalError::Prompt(format!("Invalid session ID\n")))?)
        })?;

        match session.type_ {
            SessionType::Game => {
                self.aero
                    .obtain::<Arc<GameManager>>()
                    .enter_session(
                        session.game_id.expect("Game ID must be present"),
                        session
                            .game_player_index
                            .expect("Player index must be present"),
                        self.new_session_link(),
                    )
                    .await?;
            }
            SessionType::GameProposal => {
                self.aero
                    .obtain::<Arc<ProposalManager>>()
                    .enter_session(
                        session
                            .game_proposal_id
                            .expect("Proposal ID must be present"),
                        self.user_id,
                        self.new_session_link(),
                    )
                    .await?;
            }
        }

        Ok(())
    }
    async fn handle_command_line(&mut self, line: &str) -> Result<(), TerminalError> {
        match Ui::instance().interpret_command(line)? {
            CommandInterpretation::Action { command, args } => match command.as_str() {
                "propose" => {
                    self.propose(Self::unpack_args(args)?).await?;
                }
                "sessions" => {
                    self.sessions().await?;
                }
                "enter" => {
                    self.enter(Self::unpack_args(args)?).await?;
                }
                _ => return Err(TerminalError::Prompt("Not implemented\n".into())),
            },
            CommandInterpretation::Response { prompt } => {
                self.print(prompt).await?;
            }
            CommandInterpretation::Noop => {}
        }
        Ok(())
    }
    async fn handle_line(&mut self, line: String) -> Result<(), TerminalError> {
        enum Mode<'a, 'b> {
            Command(&'a str),
            SessionCommand(&'b ActiveSession, &'a str),
        }
        let mode = if let Some(line) = line.strip_prefix(".") {
            Mode::Command(line)
        } else if let Some(session) = &self.active_session {
            Mode::SessionCommand(session, &line)
        } else {
            Mode::Command(&line)
        };
        match mode {
            Mode::Command(line) => self.handle_command_line(line).await?,
            Mode::SessionCommand(session, line) => {
                if session
                    .tx
                    .send(ClientSessionCommand::TerminalLine(line.into()))
                    .await
                    .is_err()
                {
                    self.print("Session closed.\n".into()).await?;
                    self.active_session = None;
                }
            }
        }

        Ok(())
    }
    async fn recv_event(&mut self) -> anyhow::Result<TerminalSessionEvent> {
        Ok(if let Some(session) = &mut self.active_session {
            tokio::select! {
                client_cmd = self.rx.recv() => {
                    TerminalSessionEvent::ClientCommand(client_cmd)
                },
                session_cmd = session.rx.recv() => {
                    TerminalSessionEvent::SessionCommand(session_cmd)
                }
            }
        } else {
            TerminalSessionEvent::ClientCommand(self.rx.recv().await)
        })
    }
    async fn request_line(&self) -> anyhow::Result<()> {
        self.tx
            .send(TerminalServerCommand::RequestLine {
                prompt: "> ".into(),
            })
            .await?;
        Ok(())
    }
    async fn run_inner(&mut self) -> anyhow::Result<()> {
        let mut requested_line = false;
        let mut expecting_line = false;
        loop {
            if self.active_session.is_none() {
                expecting_line = true;
            }
            if expecting_line && !requested_line {
                self.request_line().await?;
                requested_line = true;
            }
            let event = self.recv_event().await?;
            match event {
                TerminalSessionEvent::ClientCommand(Some(TerminalClientCommand::Line(line))) => {
                    requested_line = false;
                    expecting_line = false;
                    match self.handle_line(line).await {
                        Ok(()) => {}
                        Err(TerminalError::Internal(e)) => break Err(e),
                        Err(TerminalError::Prompt(text)) => self.print(text).await?,
                    }
                }
                TerminalSessionEvent::ClientCommand(None) => {
                    break Ok(());
                }
                TerminalSessionEvent::SessionCommand(Some(
                    ServerSessionCommand::TerminalPrint(text),
                )) => {
                    self.print(text).await?;
                }
                TerminalSessionEvent::SessionCommand(Some(
                    ServerSessionCommand::TerminalRequestLine,
                )) => {
                    requested_line = true;
                }
                TerminalSessionEvent::SessionCommand(None) => {
                    self.active_session = None;
                }
            }
        }
    }
}
