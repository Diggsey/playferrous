use std::{io, sync::Arc};

use async_trait::async_trait;
use playferrous_presentation::{Presentation, UserManagement};
use russh::MethodSet;
use serde::{Deserialize, Serialize};

mod data_writer;
mod error;
use error::Error;
mod client;
mod data_reader;
mod handler;
mod null_buf;

const fn default_port() -> u16 {
    9000
}

fn default_key_path() -> String {
    "server_key.p8".into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_key_path")]
    key_path: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: default_port(),
            key_path: default_key_path(),
        }
    }
}

pub struct PresentationSsh {}

impl PresentationSsh {
    async fn load_or_generate_key(config: &Config) -> Result<russh_keys::key::KeyPair, Error> {
        match tokio::fs::read(&config.key_path).await {
            Ok(key_data) => Ok(russh_keys::pkcs8::decode_pkcs8(&key_data, None)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let key = russh_keys::key::KeyPair::generate_ed25519().unwrap();
                let key_data = russh_keys::pkcs8::encode_pkcs8(&key);
                tokio::fs::write(&config.key_path, key_data).await?;
                Ok(key)
            }
            Err(e) => Err(e.into()),
        }
    }
}

#[async_trait]
impl Presentation for PresentationSsh {
    type Config = Config;
    type Error = Error;

    async fn new(config: &Config, user_management: Arc<dyn UserManagement>) -> Result<Self, Error> {
        let key = Self::load_or_generate_key(&config).await?;
        let mut ssh_config = russh::server::Config {
            auth_rejection_time: std::time::Duration::from_millis(200),
            auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
            keys: vec![key],
            // This is actually an inactivity timeout
            connection_timeout: None,
            ..Default::default()
        };
        ssh_config.methods = MethodSet::PUBLICKEY | MethodSet::PASSWORD;
        let server = Server::new(user_management);

        russh::server::run(Arc::new(ssh_config), ("0.0.0.0", config.port), server)
            .await
            .map_err(Error::FailedToStart)?;
        Ok(Self {})
    }
}

struct Server {
    user_management: Arc<dyn UserManagement>,
}

impl Server {
    fn new(user_management: Arc<dyn UserManagement>) -> Self {
        Self { user_management }
    }
}

impl russh::server::Server for Server {
    type Handler = handler::Handler;

    fn new_client(&mut self, _peer_addr: Option<std::net::SocketAddr>) -> Self::Handler {
        handler::Handler::new(self.user_management.clone())
    }
}
