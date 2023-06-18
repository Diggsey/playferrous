use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::sync::mpsc;

#[async_trait]
pub trait Presentation: Sized {
    type Error: Error;
    type Config: DeserializeOwned;
    async fn new(
        config: Self::Config,
        user_management: Arc<dyn UserManagement>,
    ) -> Result<Self, Self::Error>;
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct UserId(pub i64);

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
