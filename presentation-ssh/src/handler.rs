use std::{fmt, sync::Arc};

use async_trait::async_trait;
use russh::{ChannelId, MethodSet};

use playferrous_presentation::{UserId, UserManagement, UserManagementError};
use tokio::sync::mpsc;
use tracing::{error, instrument};

use crate::{client, data_reader::DataReader, data_writer::DataWriter, error::Error};

#[derive(Debug)]
pub(crate) enum AuthState {
    Unauthenticated,
    Attempted {
        username: String,
        password: String,
        reenter_password: Vec<u8>,
    },
    Authenticated {
        user_id: UserId,
    },
}

pub(crate) struct Handler {
    auth_state: AuthState,
    auth_key_fingerprint: Option<String>,
    user_management: Arc<dyn UserManagement>,
    session: Option<(russh::server::Handle, ChannelId)>,
    data_stream: Option<mpsc::Sender<Vec<u8>>>,
}

impl fmt::Debug for Handler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Handler")
            .field("auth_state", &self.auth_state)
            .field("auth_key_fingerprint", &self.auth_key_fingerprint)
            .field("data_stream", &self.data_stream)
            .finish_non_exhaustive()
    }
}

impl Handler {
    pub fn new(user_management: Arc<dyn UserManagement>) -> Self {
        Self {
            auth_state: AuthState::Unauthenticated,
            auth_key_fingerprint: None,
            user_management,
            session: None,
            data_stream: None,
        }
    }
    #[instrument(skip(self))]
    async fn auth_success(&mut self, user_id: UserId) -> Result<(), Error> {
        self.auth_state = AuthState::Authenticated { user_id };
        if let Some(fingerprint) = &self.auth_key_fingerprint {
            self.user_management
                .add_user_public_key(user_id, fingerprint)
                .await?;
        }
        Ok(())
    }
    #[instrument(skip(self))]
    async fn connect(&mut self, user_id: UserId) -> Result<(), Error> {
        let (session, channel) = self
            .session
            .clone()
            .expect("Should not try to connect without channel");
        let terminal_connection = self.user_management.connect_terminal(user_id).await?;
        let (tx, rx) = mpsc::channel(4);
        self.data_stream = Some(tx);

        tokio::spawn(async move {
            let res = client::run(
                terminal_connection,
                DataReader::new(rx),
                DataWriter::new(session.clone(), channel),
            )
            .await;
            if let Err(e) = res {
                error!("Client task failed: {}", e)
            }
        });
        Ok(())
    }
}

#[async_trait]
impl russh::server::Handler for Handler {
    type Error = Error;

    #[instrument(skip(self))]
    async fn auth_publickey(
        mut self,
        username: &str,
        public_key: &russh_keys::key::PublicKey,
    ) -> Result<(Self, russh::server::Auth), Error> {
        let fingerprint = public_key.fingerprint();
        match self
            .user_management
            .login_user_with_public_key(username, &fingerprint)
            .await
        {
            Ok(user_id) => {
                self.auth_success(user_id).await?;
                Ok((self, russh::server::Auth::Accept))
            }
            Err(UserManagementError::UserDoesNotExist | UserManagementError::InvalidAuth) => {
                self.auth_key_fingerprint = Some(fingerprint);
                Ok((
                    self,
                    russh::server::Auth::Reject {
                        proceed_with_methods: Some(MethodSet::PASSWORD | MethodSet::PUBLICKEY),
                    },
                ))
            }
            Err(e) => Err(e.into()),
        }
    }

    #[instrument(skip(self))]
    async fn auth_password(
        mut self,
        username: &str,
        password: &str,
    ) -> Result<(Self, russh::server::Auth), Error> {
        match self
            .user_management
            .login_user_with_password(username, password)
            .await
        {
            Ok(user_id) => {
                self.auth_success(user_id).await?;
                Ok((self, russh::server::Auth::Accept))
            }
            Err(UserManagementError::InvalidAuth) => Ok((
                self,
                russh::server::Auth::Reject {
                    proceed_with_methods: Some(MethodSet::PASSWORD | MethodSet::PUBLICKEY),
                },
            )),
            Err(UserManagementError::UserDoesNotExist) => {
                self.auth_state = AuthState::Attempted {
                    username: username.into(),
                    password: password.into(),
                    reenter_password: Vec::new(),
                };
                Ok((self, russh::server::Auth::Accept))
            }
            Err(e) => Err(e.into()),
        }
    }

    #[instrument(skip(self, session))]
    async fn channel_open_session(
        mut self,
        channel: russh::Channel<russh::server::Msg>,
        session: russh::server::Session,
    ) -> Result<(Self, bool, russh::server::Session), Error> {
        dbg!();
        let allow = self.session.is_none()
            && match &self.auth_state {
                AuthState::Unauthenticated => false,
                AuthState::Authenticated { .. } | AuthState::Attempted { .. } => true,
            };
        if allow {
            self.session = Some((session.handle(), channel.id()));
        }
        Ok((self, allow, session))
    }

    #[instrument(level = "debug", skip(self, session), fields(data=std::str::from_utf8(data).ok()))]
    async fn data(
        mut self,
        channel: ChannelId,
        mut data: &[u8],
        mut session: russh::server::Session,
    ) -> Result<(Self, russh::server::Session), Error> {
        // Disconnect on Ctrl+C or Ctrl+D
        if data.contains(&3) || data.contains(&4) {
            session.close(channel);
            return Ok((self, session));
        }
        if let AuthState::Attempted {
            username,
            password,
            reenter_password,
        } = &mut self.auth_state
        {
            while let Some((&hd, tl)) = data.split_first() {
                data = tl;
                match hd {
                    // Return
                    13 => {
                        let success = if let Ok(password2) =
                            String::from_utf8(reenter_password.split_off(0))
                        {
                            *password == password2
                        } else {
                            false
                        };
                        if success {
                            let user_id =
                                self.user_management.create_user(username, password).await?;
                            self.auth_success(user_id).await?;
                            self.connect(user_id).await?;
                            break;
                        } else {
                            let message = format!("Passwords did not match.\r\n");
                            session.data(channel, message.into());
                            session.close(channel);
                        }
                    }
                    // Backspace
                    8 => {
                        while let Some(c) = reenter_password.pop() {
                            if c < 0x80 || c >= 0xC0 {
                                break;
                            }
                        }
                    }
                    _ => reenter_password.push(hd),
                }
            }
        }
        if !data.is_empty() {
            if let Some(tx) = &self.data_stream {
                if let Err(e) = tx.send(data.into()).await {
                    eprintln!("{}", e);
                    session.close(channel);
                }
            }
        }
        Ok((self, session))
    }

    #[instrument(skip(self, session))]
    async fn pty_request(
        mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        mut session: russh::server::Session,
    ) -> Result<(Self, russh::server::Session), Error> {
        match &self.auth_state {
            AuthState::Attempted {
                username, password, ..
            } => {
                if password.len() < 8 {
                    let message = format!("Password must be at least 8 characters long.\r\n");
                    session.data(channel, message.into());
                    session.close(channel);
                } else {
                    let message = format!(
                        "User `{username}` does not exist. Re-enter password to create user:\r\n"
                    );
                    session.data(channel, message.into());
                }
            }
            AuthState::Authenticated { user_id } => {
                self.connect(*user_id).await?;
            }
            AuthState::Unauthenticated => {}
        }

        Ok((self, session))
    }
}
