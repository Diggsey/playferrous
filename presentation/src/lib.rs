use std::{error::Error, fmt::Debug, num::ParseIntError, sync::Arc};

use async_trait::async_trait;
use bichannel::Bichannel;
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

pub mod actor;
pub mod bichannel;
pub mod terminal;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PresentationKind {
    Terminal,
    Graphical,
}

#[async_trait]
pub trait Presentation: Sized {
    type Error: Error;
    type Config: Debug + Serialize + DeserializeOwned;
    async fn new(
        config: &Self::Config,
        user_management: Arc<dyn UserManagement>,
    ) -> Result<Self, Self::Error>;
}

#[derive(Debug, Copy, Clone, Error)]
#[error("Invalid ID")]
pub struct InvalidIdError;

impl From<ParseIntError> for InvalidIdError {
    fn from(_value: ParseIntError) -> Self {
        Self
    }
}

macro_rules! declare_ids {
    ($($name:ident => $prefix:literal,)*) => {
        $(
            #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, sqlx::Type)]
            #[sqlx(transparent)]
            pub struct $name(pub i64);

            impl std::fmt::Display for $name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}{}", $prefix, self.0)
                }
            }

            impl std::str::FromStr for $name {
                type Err = InvalidIdError;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    Ok(Self(s.strip_prefix($prefix).ok_or(InvalidIdError)?.parse()?))
                }
            }

            impl sqlx::postgres::PgHasArrayType for $name {
                fn array_type_info() -> sqlx::postgres::PgTypeInfo {
                    <i64 as sqlx::postgres::PgHasArrayType>::array_type_info()
                }
            }
        )*
    };
}

declare_ids! {
    UserId => "u",
    GameId => "g",
    GameProposalId => "p",
    SessionId => "s",
    RequestId => "r",
    MessageId => "m",
    GroupId => "o",
}

#[derive(Debug, Error)]
pub enum UserManagementError {
    #[error("User does not exist")]
    UserDoesNotExist,
    #[error("User could not be authenticated")]
    InvalidAuth,
    #[error("User already exists")]
    UserAlreadyExists,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[async_trait]
pub trait UserManagement: Send + Sync {
    async fn login_user_with_password(
        &self,
        username: &str,
        password: &str,
    ) -> Result<UserId, UserManagementError>;
    async fn login_user_with_public_key(
        &self,
        username: &str,
        fingerprint: &str,
    ) -> Result<UserId, UserManagementError>;
    async fn create_user(
        &self,
        username: &str,
        password: &str,
    ) -> Result<UserId, UserManagementError>;
    async fn add_user_public_key(
        &self,
        user_id: UserId,
        fingerprint: &str,
    ) -> Result<(), UserManagementError>;
    async fn connect(
        &self,
        user_id: UserId,
        kind: PresentationKind,
    ) -> anyhow::Result<Bichannel<PresentationToConnectionMsg, ConnectionToPresentationMsg>>;
}

#[derive(Debug, Clone)]
pub struct CreateGameProposal {
    pub game_type: String,
}

#[derive(Debug, Clone)]
pub enum PresentationToConnectionMsg {
    ListGames,
    ListProposals,
    ListSessions,
    ListMessages,
    Propose(CreateGameProposal),
    Withdraw(GameProposalId),
    Enter(SessionId),
    Exit,
    SessionCommand(SessionCommand),
}

#[derive(Debug, Clone)]
pub enum SessionCommand {
    Terminal(TerminalSessionCommand),
}

#[derive(Debug, Clone)]
pub enum TerminalSessionCommand {
    Line(String),
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: SessionId,
    pub kind: SessionKind,
}

#[derive(Debug, Clone)]
pub enum SessionKind {
    GameProposal(GameProposalId),
    Game(GameId),
}

#[derive(Debug, Clone)]
pub enum SessionEvent {
    Terminal(TerminalSessionEvent),
}

#[derive(Debug, Clone)]
pub enum TerminalSessionEvent {
    Line(String),
}

#[derive(Debug, Clone)]
pub struct MessageMin {
    pub id: MessageId,
    pub sent_at: DateTime<Utc>,
    pub subject: String,
    pub from: Option<UserMin>,
    pub request_id: Option<RequestId>,
}

#[derive(Debug, Clone)]
pub struct GameProposalMin {
    pub id: GameProposalId,
    pub created_at: DateTime<Utc>,
    pub game_type: String,
}

#[derive(Debug, Clone)]
pub struct SessionMin {
    pub id: SessionId,
    pub created_at: DateTime<Utc>,
    pub kind: SessionKind,
}

#[derive(Debug, Clone)]
pub struct UserMin {
    pub id: UserId,
    pub username: String,
}

#[derive(Debug, Clone)]
pub enum ConnectionToPresentationMsg {
    MessageList(Vec<MessageMin>),
    ProposalList(Vec<GameProposalMin>),
    SessionList(Vec<SessionMin>),
    EnteredSession(SessionInfo),
    ExitedSession,
    SessionEvent(SessionEvent),
    Error(String),
}
