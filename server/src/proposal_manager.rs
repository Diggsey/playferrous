use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};

use aerosol::{Aerosol, Constructible};
use futures::{future::BoxFuture, FutureExt};
use playferrous_presentation::{GameProposalId, UserId};
use tokio::sync::mpsc;

use crate::active_session::{ClientSessionCommand, ServerSessionCommand, SessionLink};

struct ProposalEvent {
    user_id: UserId,
    event_type: ProposalEventType,
}

enum ProposalEventType {
    Enter(mpsc::Sender<ServerSessionCommand>),
    Leave,
    Command(ClientSessionCommand),
}

#[derive(Debug)]
struct ProposalHandle {
    tx: mpsc::Sender<ProposalEvent>,
}

#[derive(Debug)]
pub struct ProposalManager {
    proposals: Arc<Mutex<HashMap<GameProposalId, ProposalHandle>>>,
    aero: Aerosol,
}

impl Constructible for ProposalManager {
    type Error = Infallible;
    fn construct(aero: &Aerosol) -> Result<Self, Self::Error> {
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
        mut link: SessionLink,
    ) -> anyhow::Result<()> {
        let tx = {
            let mut guard = self.proposals.lock().expect("Lock to not be poisoned...");
            guard
                .entry(proposal_id)
                .or_insert_with(|| self.start_proposal(proposal_id))
                .tx
                .clone()
        };
        tokio::spawn(async move {
            if tx
                .send(ProposalEvent {
                    user_id,
                    event_type: ProposalEventType::Enter(link.tx),
                })
                .await
                .is_err()
            {
                return;
            }
            while let Some(cmd) = link.rx.recv().await {
                if tx
                    .send(ProposalEvent {
                        user_id,
                        event_type: ProposalEventType::Command(cmd),
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            if tx
                .send(ProposalEvent {
                    user_id,
                    event_type: ProposalEventType::Leave,
                })
                .await
                .is_err()
            {
                return;
            }
        });
        Ok(())
    }

    fn start_proposal(&self, proposal_id: GameProposalId) -> ProposalHandle {
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(
            Proposal {
                proposal_id,
                rx,
                proposals: self.proposals.clone(),
                users: Default::default(),
            }
            .run(),
        );
        ProposalHandle { tx }
    }
}

struct Proposal {
    proposal_id: GameProposalId,
    rx: mpsc::Receiver<ProposalEvent>,
    proposals: Arc<Mutex<HashMap<GameProposalId, ProposalHandle>>>,
    users: HashMap<UserId, mpsc::Sender<ServerSessionCommand>>,
}

const USER_TIMEOUT: Duration = Duration::from_millis(200);

impl Proposal {
    fn timeout_user(&mut self, user_id: UserId) -> BoxFuture<()> {
        async move {
            self.users.remove(&user_id);
            tracing::info!("User {} left due to a timeout.", user_id);
            self.broadcast(ServerSessionCommand::TerminalPrint(format!(
                "User {} left due to a timeout.\n",
                user_id
            )))
            .await;
        }
        .boxed()
    }
    async fn send_to_user(&mut self, user_id: UserId, cmd: ServerSessionCommand) {
        if let Some(tx) = self.users.get_mut(&user_id) {
            if tx.send_timeout(cmd, USER_TIMEOUT).await.is_err() {
                self.timeout_user(user_id).await;
            }
        }
    }
    async fn broadcast(&mut self, cmd: ServerSessionCommand) {
        let user_ids: Vec<_> = self.users.keys().copied().collect();
        for user_id in user_ids {
            self.send_to_user(user_id, cmd.clone()).await;
        }
    }
    async fn run(mut self) {
        tracing::info!("Running proposal {}", self.proposal_id);
        while let Some(event) = self.rx.recv().await {
            match event.event_type {
                ProposalEventType::Enter(tx) => {
                    tracing::info!("User {} entered.", event.user_id);
                    self.broadcast(ServerSessionCommand::TerminalPrint(format!(
                        "User {} entered.\n",
                        event.user_id
                    )))
                    .await;
                    self.users.insert(event.user_id, tx);
                    self.send_to_user(
                        event.user_id,
                        ServerSessionCommand::TerminalPrint("Welcome to session.\n".into()),
                    )
                    .await;
                    self.send_to_user(event.user_id, ServerSessionCommand::TerminalRequestLine)
                        .await;
                }
                ProposalEventType::Command(cmd) => match cmd {
                    ClientSessionCommand::TerminalLine(text) => {
                        self.broadcast(ServerSessionCommand::TerminalPrint(text))
                            .await;
                        self.send_to_user(event.user_id, ServerSessionCommand::TerminalRequestLine)
                            .await;
                    }
                },
                ProposalEventType::Leave => {
                    if self.users.remove(&event.user_id).is_some() {
                        tracing::info!("User {} left.", event.user_id);
                        self.broadcast(ServerSessionCommand::TerminalPrint(format!(
                            "User {} left.\n",
                            event.user_id
                        )))
                        .await;
                    }
                }
            }
        }
    }
}

impl Drop for Proposal {
    fn drop(&mut self) {
        self.proposals
            .lock()
            .expect("Mutex to not be poisoned")
            .remove(&self.proposal_id);
    }
}
