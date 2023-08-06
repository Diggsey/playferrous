use std::sync::Arc;

use aerosol::{Aero, AsyncConstructible};
use async_trait::async_trait;
use launchers::AnyLauncherConfig;
use presentations::{AnyPresentationConfig, Presentations};
use serde::{Deserialize, Serialize};
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

use crate::{connection_manager::ConnectionManager, user_management::UserManagementImpl};

#[macro_use]
mod database;
mod connection_manager;
mod game_manager;
mod launchers;
mod presentations;
mod proposal_manager;
mod user_management;

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    launcher: Vec<AnyLauncherConfig>,
    presentation: Vec<AnyPresentationConfig>,
}

#[async_trait]
impl AsyncConstructible for Config {
    type Error = anyhow::Error;
    async fn construct_async(_aero: &Aero) -> Result<Self, Self::Error> {
        let config_str = tokio::fs::read_to_string("playferrous.toml").await?;
        Ok(toml::from_str(&config_str)?)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenv::dotenv();
    tracing_subscriber::fmt()
        .with_span_events(FmtSpan::ACTIVE)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let aero = Aero::new()
        .with_constructed::<Arc<UserManagementImpl>>()
        .with_constructed::<ConnectionManager>()
        .with_constructed_async::<Arc<Presentations>>()
        .await;

    aero.get::<ConnectionManager, _>()
        .broadcast([], |_| panic!())
        .await;

    println!("Started...");
    Ok(())
}
