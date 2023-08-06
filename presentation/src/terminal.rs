use async_trait::async_trait;
use thiserror::Error;
use tracing::error;

use crate::{
    actor::Actor,
    bichannel::{bichannel, Bichannel},
    select_recv_loop, ConnectionToPresentationMsg, CreateGameProposal, GameProposalMin,
    InvalidIdError, MessageMin, PresentationKind, PresentationToConnectionMsg, SessionCommand,
    SessionEvent, SessionInfo, SessionMin, TerminalSessionCommand, TerminalSessionEvent, UserId,
    UserManagement,
};

use self::ui::{CommandInterpretation, Ui};

mod ui;

#[derive(Debug, Clone)]
pub enum PresentationToTerminalMsg {
    PrintLine(String),
    ErrorLine(String),
}

#[derive(Debug, Clone)]
pub enum TerminalToPresentationMsg {
    ReadLine(String),
}

pub struct TerminalPresentation {
    terminal_channel: Bichannel<PresentationToTerminalMsg, TerminalToPresentationMsg>,
    connection_channel: Bichannel<PresentationToConnectionMsg, ConnectionToPresentationMsg>,
    active_session: Option<SessionInfo>,
}

#[derive(Debug, Error)]
enum TerminalError {
    #[error("Client disconnected")]
    Disconnected,
    #[error("Print: {0}")]
    Print(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<InvalidIdError> for TerminalError {
    fn from(value: InvalidIdError) -> Self {
        Self::Print(value.to_string())
    }
}

#[async_trait]
impl Actor for TerminalPresentation {
    async fn run(mut self) -> anyhow::Result<()> {
        select_recv_loop! {
            msg = self.connection_channel.r.recv() => {
                let res = self.handle_connection_msg(msg).await;
                self.handle_error(res).await?;
            },
            msg = self.terminal_channel.r.recv() => {
                let res = self.handle_terminal_msg(msg).await;
                self.handle_error(res).await?;
            },
        }
        Ok(())
    }
}

impl TerminalPresentation {
    async fn handle_error(&mut self, res: Result<(), TerminalError>) -> anyhow::Result<()> {
        match res {
            Ok(()) => Ok(()),
            Err(TerminalError::Print(line)) => {
                self.terminal_channel
                    .s
                    .send(PresentationToTerminalMsg::ErrorLine(line))
                    .await?;
                Ok(())
            }
            Err(TerminalError::Disconnected) => Ok(()),
            Err(TerminalError::Internal(e)) => Err(e),
        }
    }
    pub async fn connect(
        user_management: &dyn UserManagement,
        user_id: UserId,
    ) -> anyhow::Result<Bichannel<TerminalToPresentationMsg, PresentationToTerminalMsg>> {
        let connection_channel = user_management
            .connect(user_id, PresentationKind::Terminal)
            .await?;
        let (terminal_channel, presentation_channel) = bichannel(1);
        Self {
            terminal_channel,
            connection_channel,
            active_session: None,
        }
        .spawn();
        Ok(presentation_channel)
    }
    async fn send_to_terminal(
        &mut self,
        msg: PresentationToTerminalMsg,
    ) -> Result<(), TerminalError> {
        self.terminal_channel
            .s
            .send(msg)
            .await
            .map_err(|_| TerminalError::Disconnected)
    }
    async fn send_to_connection(
        &mut self,
        msg: PresentationToConnectionMsg,
    ) -> Result<(), TerminalError> {
        self.connection_channel
            .s
            .send(msg)
            .await
            .map_err(|_| TerminalError::Print("Server closed the connection".into()))
    }
    async fn println(&mut self, line: String) -> Result<(), TerminalError> {
        self.send_to_terminal(PresentationToTerminalMsg::PrintLine(line))
            .await
    }
    fn unpack_args<const N: usize>(args: Vec<String>) -> Result<[String; N], TerminalError> {
        args.try_into().map_err(|args: Vec<_>| {
            TerminalError::Print(format!(
                "Expected {} arguments, received {}!\n",
                N,
                args.len()
            ))
        })
    }
    async fn propose(&mut self, [game_type]: [String; 1]) -> Result<(), TerminalError> {
        self.send_to_connection(PresentationToConnectionMsg::Propose(CreateGameProposal {
            game_type,
        }))
        .await
    }
    async fn sessions(&mut self, []: [String; 0]) -> Result<(), TerminalError> {
        self.send_to_connection(PresentationToConnectionMsg::ListSessions)
            .await
    }
    async fn enter(&mut self, [session_id]: [String; 1]) -> Result<(), TerminalError> {
        self.send_to_connection(PresentationToConnectionMsg::Enter(session_id.parse()?))
            .await
    }
    async fn exit(&mut self, []: [String; 0]) -> Result<(), TerminalError> {
        self.send_to_connection(PresentationToConnectionMsg::Exit)
            .await
    }
    async fn proposals(&mut self, []: [String; 0]) -> Result<(), TerminalError> {
        self.send_to_connection(PresentationToConnectionMsg::ListProposals)
            .await
    }
    async fn messages(&mut self, []: [String; 0]) -> Result<(), TerminalError> {
        self.send_to_connection(PresentationToConnectionMsg::ListMessages)
            .await
    }
    async fn handle_message_list(
        &mut self,
        messages: Vec<MessageMin>,
    ) -> Result<(), TerminalError> {
        for message in messages {
            self.println(format!(
                "{:>6} {:12?} {:12?} {:>6} {}\n",
                message.id,
                message.sent_at,
                message
                    .from
                    .map(|u| u.username)
                    .unwrap_or_else(|| "System".into()),
                message
                    .request_id
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                message.subject
            ))
            .await?;
        }
        Ok(())
    }
    async fn handle_proposal_list(
        &mut self,
        proposals: Vec<GameProposalMin>,
    ) -> Result<(), TerminalError> {
        for proposal in proposals {
            self.println(format!(
                "{:>6} {:12?} {}\n",
                proposal.id, proposal.game_type, proposal.created_at
            ))
            .await?;
        }
        Ok(())
    }
    async fn handle_session_list(
        &mut self,
        sessions: Vec<SessionMin>,
    ) -> Result<(), TerminalError> {
        for session in sessions {
            self.println(format!(
                "{:>6} {:12?} {}\n",
                session.id, session.type_, session.created_at
            ))
            .await?;
        }
        Ok(())
    }
    async fn handle_command_line(&mut self, line: &str) -> Result<(), TerminalError> {
        match Ui::instance().interpret_command(line)? {
            CommandInterpretation::Action { command, args } => match command.as_str() {
                "propose" => {
                    self.propose(Self::unpack_args(args)?).await?;
                }
                "proposals" => {
                    self.proposals(Self::unpack_args(args)?).await?;
                }
                "messages" => {
                    self.messages(Self::unpack_args(args)?).await?;
                }
                "sessions" => {
                    self.sessions(Self::unpack_args(args)?).await?;
                }
                "enter" => {
                    self.enter(Self::unpack_args(args)?).await?;
                }
                "exit" => {
                    self.exit(Self::unpack_args(args)?).await?;
                }
                _ => return Err(TerminalError::Print("Not implemented\n".into())),
            },
            CommandInterpretation::Response { prompt } => {
                self.send_to_terminal(PresentationToTerminalMsg::PrintLine(prompt))
                    .await?;
            }
            CommandInterpretation::Noop => {}
        }
        Ok(())
    }
    async fn handle_read_line(&mut self, line: String) -> Result<(), TerminalError> {
        enum Mode<'a> {
            Command(&'a str),
            SessionCommand(&'a str),
        }
        let mode = if let Some(line) = line.strip_prefix("/") {
            Mode::Command(line)
        } else if self.active_session.is_some() {
            Mode::SessionCommand(&line)
        } else {
            Mode::Command(&line)
        };
        match mode {
            Mode::Command(line) => self.handle_command_line(line).await?,
            Mode::SessionCommand(line) => {
                self.send_to_connection(PresentationToConnectionMsg::SessionCommand(
                    SessionCommand::Terminal(TerminalSessionCommand::Line(line.into())),
                ))
                .await?;
            }
        }

        Ok(())
    }
    async fn handle_terminal_msg(
        &mut self,
        msg: TerminalToPresentationMsg,
    ) -> Result<(), TerminalError> {
        match msg {
            TerminalToPresentationMsg::ReadLine(line) => self.handle_read_line(line).await,
        }
    }
    async fn handle_connection_msg(
        &mut self,
        msg: ConnectionToPresentationMsg,
    ) -> Result<(), TerminalError> {
        match msg {
            ConnectionToPresentationMsg::EnteredSession(session) => {
                self.active_session = Some(session);
            }
            ConnectionToPresentationMsg::ExitedSession => {
                self.active_session = None;
                self.println("Exited session".into()).await?;
            }
            ConnectionToPresentationMsg::SessionEvent(SessionEvent::Terminal(ev)) => {
                self.handle_session_event(ev).await?
            }
            ConnectionToPresentationMsg::Error(e) => return Err(TerminalError::Print(e)),
            ConnectionToPresentationMsg::MessageList(messages) => {
                self.handle_message_list(messages).await?
            }
            ConnectionToPresentationMsg::ProposalList(proposals) => {
                self.handle_proposal_list(proposals).await?
            }
            ConnectionToPresentationMsg::SessionList(sessions) => {
                self.handle_session_list(sessions).await?
            }
        }
        Ok(())
    }

    async fn handle_session_event(
        &mut self,
        ev: TerminalSessionEvent,
    ) -> Result<(), TerminalError> {
        match ev {
            TerminalSessionEvent::Line(line) => self.println(line).await,
        }
    }
}
