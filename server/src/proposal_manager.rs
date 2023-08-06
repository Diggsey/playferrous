use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};

use aerosol::{Aero, Constructible};
use async_trait::async_trait;
use dashmap::DashMap;
use futures::{future::BoxFuture, FutureExt};
use playferrous_presentation::{
    actor::Actor,
    bichannel::{bichannel, Bichannel},
    GameProposalId, PresentationKind, SessionCommand, SessionEvent, TerminalSessionCommand,
    TerminalSessionEvent, UserId,
};
use tokio::sync::mpsc;

use crate::{
    connection_manager::{ConnectionToSessionMsg, SessionMember, SessionToConnectionMsg},
    utils::{FutureExt2, FutureIteratorExt},
};

#[derive(Debug)]
struct EnterProposalSession {
    user_id: UserId,
    bichannel: Bichannel<SessionToConnectionMsg, ConnectionToSessionMsg>,
    kind: PresentationKind,
}

#[derive(Debug)]
enum SystemToProposalMsg {
    Enter(EnterProposalSession),
}

#[derive(Debug)]
struct Proposal {
    s: mpsc::Sender<SystemToProposalMsg>,
}

#[derive(Debug, Clone)]
pub struct ProposalManager {
    proposals: Arc<DashMap<GameProposalId, Proposal>>,
    aero: Aero,
}

impl Constructible for ProposalManager {
    type Error = Infallible;
    fn construct(aero: &Aero) -> Result<Self, Self::Error> {
        Ok(Self {
            proposals: Default::default(),
            aero: aero.clone(),
        })
    }
}

impl ProposalManager {
    pub async fn enter_session(
        &self,
        proposal_id: GameProposalId,
        user_id: UserId,
        kind: PresentationKind,
    ) -> anyhow::Result<Bichannel<ConnectionToSessionMsg, SessionToConnectionMsg>> {
        let s = {
            self.proposals
                .entry(proposal_id)
                .or_insert_with(|| self.start_proposal(proposal_id))
                .s
                .clone()
        };
        let (session_bichannel, connection_bichannel) = bichannel(4);
        s.send(SystemToProposalMsg::Enter(EnterProposalSession {
            user_id,
            bichannel: connection_bichannel,
            kind,
        }))
        .await?;
        Ok(session_bichannel)
    }

    fn start_proposal(&self, proposal_id: GameProposalId) -> Proposal {
        let (system_s, system_r) = mpsc::channel(4);
        ProposalActor {
            aero: self.aero.clone(),
            proposal_id,
            system_r,
            connections: Default::default(),
        }
        .spawn();
        Proposal { s: system_s }
    }
}

#[derive(Debug)]
struct Connection {
    #[allow(unused)]
    kind: PresentationKind,
    bichannel: Bichannel<SessionToConnectionMsg, ConnectionToSessionMsg>,
}

#[derive(Debug)]
struct ProposalActor {
    aero: Aero,
    proposal_id: GameProposalId,
    system_r: mpsc::Receiver<SystemToProposalMsg>,
    connections: HashMap<UserId, Connection>,
}

const USER_TIMEOUT: Duration = Duration::from_millis(200);

#[async_trait]
impl Actor for ProposalActor {
    async fn run(mut self) -> anyhow::Result<()> {
        tracing::info!("Running proposal {}", self.proposal_id);
        loop {
            tokio::select! {
                biased;
                maybe_msg = self.system_r.recv() => if let Some(msg) = maybe_msg { self.handle_system_msg(msg).await? } else {break},
                (user_id, maybe_msg) = self.connections.iter_mut().map(|(user_id, conn)| conn.bichannel.r.recv().with_key(*user_id)).select() => {
                    if let Some(msg) = maybe_msg {
                        self.handle_connection_msg(user_id, msg).await?;
                    } else {
                        self.disconnect_user(user_id).await;
                    }
                },
                _ = tokio::time::sleep(Duration::from_secs(1)), if self.connections.is_empty() => {
                    break;
                }
            }
        }
        tracing::info!("Stopping proposal {}", self.proposal_id);
        Ok(())
    }
}

impl ProposalActor {
    #[tracing::instrument(skip(self))]
    async fn handle_system_msg(&mut self, msg: SystemToProposalMsg) -> anyhow::Result<()> {
        match msg {
            SystemToProposalMsg::Enter(conn) => {
                tracing::info!("User {} entered.", conn.user_id);
                self.broadcast(SessionToConnectionMsg::UserEntered(SessionMember {
                    user_id: conn.user_id,
                    player_index: None,
                }))
                .await;
                self.connections.insert(
                    conn.user_id,
                    Connection {
                        kind: conn.kind,
                        bichannel: conn.bichannel,
                    },
                );
            }
        }
        Ok(())
    }
    async fn handle_terminal_cmd(
        &mut self,
        user_id: UserId,
        msg: TerminalSessionCommand,
    ) -> anyhow::Result<()> {
        match msg {
            TerminalSessionCommand::Line(line) => {
                self.broadcast(SessionToConnectionMsg::Event(SessionEvent::Terminal(
                    TerminalSessionEvent::Line(format!("{user_id}: {line}")),
                )))
                .await;
            }
        }
        Ok(())
    }
    #[tracing::instrument(skip(self))]
    async fn handle_connection_msg(
        &mut self,
        user_id: UserId,
        msg: ConnectionToSessionMsg,
    ) -> anyhow::Result<()> {
        match msg {
            ConnectionToSessionMsg::Command(SessionCommand::Terminal(cmd)) => {
                self.handle_terminal_cmd(user_id, cmd).await
            }
        }
    }
    #[tracing::instrument(skip(self))]
    async fn disconnect_user(&mut self, user_id: UserId) {
        self.connections.remove(&user_id);
        tracing::info!("User {} left.", user_id);
        self.broadcast(SessionToConnectionMsg::UserExited(SessionMember {
            user_id,
            player_index: None,
        }))
        .await;
    }
    fn timeout_user(&mut self, user_id: UserId) -> BoxFuture<()> {
        async move {
            self.connections.remove(&user_id);
            tracing::info!("User {} left due to a timeout.", user_id);
            self.broadcast(SessionToConnectionMsg::UserExited(SessionMember {
                user_id,
                player_index: None,
            }))
            .await;
        }
        .boxed()
    }
    async fn send_to_user(&mut self, user_id: UserId, cmd: SessionToConnectionMsg) {
        if let Some(conn) = self.connections.get_mut(&user_id) {
            if conn
                .bichannel
                .s
                .send_timeout(cmd, USER_TIMEOUT)
                .await
                .is_err()
            {
                self.timeout_user(user_id).await;
            }
        }
    }
    async fn broadcast(&mut self, cmd: SessionToConnectionMsg) {
        let user_ids: Vec<_> = self.connections.keys().copied().collect();
        for user_id in user_ids {
            self.send_to_user(user_id, cmd.clone()).await;
        }
    }
}

impl Drop for ProposalActor {
    fn drop(&mut self) {
        self.aero
            .obtain::<ProposalManager>()
            .proposals
            .remove(&self.proposal_id);
    }
}
