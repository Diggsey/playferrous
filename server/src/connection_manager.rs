use std::{convert::Infallible, sync::Arc, time::Duration};

use aerosol::{Aero, Constructible};
use async_trait::async_trait;
use dashmap::{mapref::entry::Entry, DashMap};
use futures::{stream::FuturesUnordered, StreamExt};
use playferrous_presentation::{
    actor::Actor,
    bichannel::{bichannel, Bichannel},
    ConnectionToPresentationMsg, CreateGameProposal, PresentationKind, PresentationToConnectionMsg,
    SessionCommand, SessionEvent, SessionId, SessionInfo, SessionKind, TerminalSessionEvent,
    UserId,
};
use thiserror::Error;
use tokio::sync::mpsc;

use crate::{
    database::{
        self,
        session::{Session, SessionType},
        TransactError,
    },
    game_manager::GameManager,
    proposal_manager::ProposalManager,
    utils::FutureIteratorExt,
};

#[derive(Debug, Clone)]
pub enum SystemToConnectionMsg {
    NewMessage,
}

#[derive(Debug)]
struct Connection {
    s: mpsc::Sender<SystemToConnectionMsg>,
}

#[derive(Debug, Clone)]
pub struct ConnectionManager {
    aero: Aero,
    connections: Arc<DashMap<UserId, Vec<Connection>>>,
}

impl Constructible for ConnectionManager {
    type Error = Infallible;
    fn construct(aero: &Aero) -> Result<Self, Self::Error> {
        Ok(Self {
            aero: aero.clone(),
            connections: Default::default(),
        })
    }
}

#[derive(Debug, Error)]
enum ConnectionError {
    #[error("Present: {0}")]
    Present(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<ConnectionError> for TransactError<ConnectionError> {
    fn from(value: ConnectionError) -> Self {
        Self::App(value)
    }
}

impl ConnectionManager {
    pub async fn open(
        &self,
        user_id: UserId,
        kind: PresentationKind,
    ) -> anyhow::Result<Bichannel<PresentationToConnectionMsg, ConnectionToPresentationMsg>> {
        let (presentation_bichannel, connection_bichannel) = bichannel(4);
        let (system_s, system_r) = mpsc::channel(4);
        self.connections
            .entry(user_id)
            .or_default()
            .push(Connection { s: system_s });
        ConnectionActor {
            aero: self.aero.clone(),
            kind,
            user_id,
            presentation_bichannel,
            system_r,
            active_session: None,
        }
        .spawn();
        Ok(connection_bichannel)
    }

    pub async fn broadcast(
        &self,
        user_ids: impl IntoIterator<Item = UserId>,
        msg_fn: impl Fn(UserId) -> SystemToConnectionMsg,
    ) {
        let senders: Vec<_> = user_ids
            .into_iter()
            .flat_map(move |user_id| {
                self.connections
                    .get(&user_id)
                    .into_iter()
                    .flat_map(move |connections| {
                        connections
                            .iter()
                            .map(move |connection| (user_id, connection.s.clone()))
                            .collect::<Vec<_>>()
                    })
            })
            .collect();
        senders
            .iter()
            .map(|(user_id, s)| s.send_timeout(msg_fn(*user_id), Duration::from_millis(500)))
            .collect::<FuturesUnordered<_>>()
            .for_each(|res| async move {
                if let Err(e) = res {
                    tracing::error!("broadcast: {}", e);
                }
            })
            .await;
    }

    pub async fn send(&self, user_id: UserId, msg: SystemToConnectionMsg) {
        self.broadcast([user_id], |_| msg.clone()).await;
    }

    fn gc(&self, user_id: UserId) {
        if let Entry::Occupied(mut occ) = self.connections.entry(user_id) {
            let vec = occ.get_mut();
            vec.retain(|conn| !conn.s.is_closed());
            if vec.is_empty() {
                occ.remove();
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConnectionToSessionMsg {
    Command(SessionCommand),
}

#[derive(Debug, Clone)]
pub struct SessionMember {
    pub user_id: UserId,
    pub player_index: Option<i64>,
}

#[derive(Debug, Clone)]
pub enum SessionToConnectionMsg {
    UserEntered(SessionMember),
    UserExited(SessionMember),
    Event(SessionEvent),
}

#[derive(Debug)]
struct ActiveSession {
    session: Session,
    bichannel: Bichannel<ConnectionToSessionMsg, SessionToConnectionMsg>,
}

struct ConnectionActor {
    aero: Aero,
    kind: PresentationKind,
    user_id: UserId,
    presentation_bichannel: Bichannel<ConnectionToPresentationMsg, PresentationToConnectionMsg>,
    system_r: mpsc::Receiver<SystemToConnectionMsg>,
    active_session: Option<ActiveSession>,
}

impl ConnectionActor {
    async fn send_to_presentation(&mut self, msg: ConnectionToPresentationMsg) {
        let _ = self.presentation_bichannel.s.send(msg).await;
    }
    async fn propose(&mut self, proposal: CreateGameProposal) -> Result<(), ConnectionError> {
        transact!(ConnectionError, self.aero, |tx| {
            database::proposal::create(tx, &proposal.game_type, self.user_id).await?;
            Ok(())
        })
    }
    async fn sessions(&mut self) -> Result<(), ConnectionError> {
        let sessions = transact!(ConnectionError, self.aero, |tx| {
            Ok(database::session::list_for_user(tx, self.user_id).await?)
        })?;
        self.send_to_presentation(ConnectionToPresentationMsg::SessionList(sessions))
            .await;
        Ok(())
    }
    async fn enter(&mut self, session_id: SessionId) -> Result<(), ConnectionError> {
        let session = transact!(ConnectionError, self.aero, |tx| {
            Ok(
                database::session::get_by_id_and_user(tx, session_id, self.user_id)
                    .await?
                    .ok_or_else(|| ConnectionError::Present(format!("Invalid session ID\n")))?,
            )
        })?;

        let (kind, bichannel) = match session.type_ {
            SessionType::Game => {
                let game_id = session.game_id.expect("Game ID must be present");
                (
                    SessionKind::Game(game_id),
                    self.aero
                        .obtain::<Arc<GameManager>>()
                        .enter_session(
                            game_id,
                            session
                                .game_player_index
                                .expect("Player index must be present"),
                            self.kind,
                        )
                        .await?,
                )
            }
            SessionType::GameProposal => {
                let proposal_id = session
                    .game_proposal_id
                    .expect("Proposal ID must be present");
                (
                    SessionKind::GameProposal(proposal_id),
                    self.aero
                        .obtain::<ProposalManager>()
                        .enter_session(proposal_id, self.user_id, self.kind)
                        .await?,
                )
            }
        };

        self.active_session = Some(ActiveSession { session, bichannel });
        self.send_to_presentation(ConnectionToPresentationMsg::EnteredSession(SessionInfo {
            id: session_id,
            kind,
        }))
        .await;

        Ok(())
    }
    async fn exit(&mut self) -> Result<(), ConnectionError> {
        if self.active_session.take().is_some() {
            self.send_to_presentation(ConnectionToPresentationMsg::ExitedSession)
                .await;
            Ok(())
        } else {
            Err(ConnectionError::Present("No active session".into()))
        }
    }
    async fn proposals(&mut self) -> Result<(), ConnectionError> {
        let proposals = transact!(ConnectionError, self.aero, |tx| {
            Ok(database::proposal::list_for_user(tx, self.user_id).await?)
        })?;
        self.send_to_presentation(ConnectionToPresentationMsg::ProposalList(proposals))
            .await;
        Ok(())
    }
    async fn messages(&mut self) -> Result<(), ConnectionError> {
        let messages = transact!(ConnectionError, self.aero, |tx| {
            Ok(database::message::list_for_user(tx, self.user_id).await?)
        })?;
        self.send_to_presentation(ConnectionToPresentationMsg::MessageList(messages))
            .await;
        Ok(())
    }
    #[tracing::instrument(skip(self))]
    async fn handle_presentation_msg(
        &mut self,
        msg: PresentationToConnectionMsg,
    ) -> Result<(), ConnectionError> {
        match msg {
            PresentationToConnectionMsg::ListGames => todo!(),
            PresentationToConnectionMsg::ListProposals => self.proposals().await?,
            PresentationToConnectionMsg::ListSessions => self.sessions().await?,
            PresentationToConnectionMsg::ListMessages => self.messages().await?,
            PresentationToConnectionMsg::Propose(proposal) => self.propose(proposal).await?,
            PresentationToConnectionMsg::Withdraw(_) => todo!(),
            PresentationToConnectionMsg::Enter(session_id) => self.enter(session_id).await?,
            PresentationToConnectionMsg::Exit => self.exit().await?,
            PresentationToConnectionMsg::SessionCommand(cmd) => {
                if let Some(session) = &mut self.active_session {
                    let _ = session
                        .bichannel
                        .s
                        .send(ConnectionToSessionMsg::Command(cmd))
                        .await;
                }
            }
        }
        Ok(())
    }
    #[tracing::instrument(skip(self))]
    async fn handle_session_msg(
        &mut self,
        msg: SessionToConnectionMsg,
    ) -> Result<(), ConnectionError> {
        match msg {
            SessionToConnectionMsg::UserEntered(_) => {
                self.send_to_presentation(ConnectionToPresentationMsg::SessionEvent(
                    SessionEvent::Terminal(TerminalSessionEvent::Line("User entered".into())),
                ))
                .await;
            }
            SessionToConnectionMsg::UserExited(_) => {
                self.send_to_presentation(ConnectionToPresentationMsg::SessionEvent(
                    SessionEvent::Terminal(TerminalSessionEvent::Line("User exited".into())),
                ))
                .await;
            }
            SessionToConnectionMsg::Event(ev) => {
                self.send_to_presentation(ConnectionToPresentationMsg::SessionEvent(ev))
                    .await;
            }
        }
        Ok(())
    }
    #[tracing::instrument(skip(self))]
    async fn handle_system_msg(
        &mut self,
        msg: SystemToConnectionMsg,
    ) -> Result<(), ConnectionError> {
        match msg {
            SystemToConnectionMsg::NewMessage => todo!(),
        }
    }
}

#[async_trait]
impl Actor for ConnectionActor {
    async fn run(mut self) -> anyhow::Result<()> {
        loop {
            let res = tokio::select! {
                biased;
                maybe_msg = self.system_r.recv() => {
                    let Some(msg) = maybe_msg else { break };
                    self.handle_system_msg(msg).await
                },
                maybe_msg = self.active_session.as_mut().map(|session| session.bichannel.r.recv()).select() => {
                    if let Some(msg) = maybe_msg {
                        self.handle_session_msg(msg).await
                    } else {
                        self.exit().await
                    }
                },
                maybe_msg = self.presentation_bichannel.r.recv() => {
                    let Some(msg) = maybe_msg else { break };
                    self.handle_presentation_msg(msg).await
                },
            };
            match res {
                Ok(()) => {}
                Err(ConnectionError::Present(e)) => {
                    self.send_to_presentation(ConnectionToPresentationMsg::Error(e))
                        .await
                }
                Err(ConnectionError::Internal(e)) => return Err(e),
            }
        }
        Ok(())
    }
}

impl Drop for ConnectionActor {
    fn drop(&mut self) {
        self.system_r.close();
        self.aero.obtain::<ConnectionManager>().gc(self.user_id);
    }
}
