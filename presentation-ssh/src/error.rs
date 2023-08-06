use std::io;

use playferrous_presentation::UserManagementError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Ssh(#[from] russh::Error),
    #[error(transparent)]
    SshKeys(#[from] russh_keys::Error),
    #[error(transparent)]
    UserManagement(#[from] UserManagementError),
    #[error("Failed to start SSH server")]
    FailedToStart(#[source] io::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
