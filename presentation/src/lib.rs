use std::{error::Error, fmt::Debug, num::ParseIntError, sync::Arc};

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;

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

        )*
    };
}

declare_ids! {
    UserId => "u",
    GameId => "g",
    GameProposalId => "p",
    SessionId => "s",
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
    async fn connect_terminal(
        &self,
        user_id: UserId,
    ) -> Result<TerminalConnection, UserManagementError>;
}

#[derive(Debug)]
pub enum TerminalClientCommand {
    Line(String),
}

#[derive(Debug)]
pub enum TerminalServerCommand {
    RequestLine { prompt: String },
    Print { text: String },
}

#[derive(Debug)]
pub struct TerminalConnection {
    pub sender: mpsc::Sender<TerminalClientCommand>,
    pub receiver: mpsc::Receiver<TerminalServerCommand>,
}
