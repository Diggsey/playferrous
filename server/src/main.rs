use std::sync::Arc;

use aerosol::{Aerosol, AsyncConstructible};
use async_trait::async_trait;
use launchers::AnyLauncherConfig;
use presentations::{AnyPresentationConfig, Presentations};
use serde::{Deserialize, Serialize};
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

use crate::user_management::UserManagementImpl;

#[macro_use]
mod database;
mod active_session;
mod game_manager;
mod launchers;
mod presentations;
mod proposal_manager;
mod terminal_session;
mod user_management;

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    launcher: Vec<AnyLauncherConfig>,
    presentation: Vec<AnyPresentationConfig>,
}

#[async_trait]
impl AsyncConstructible for Config {
    type Error = anyhow::Error;
    async fn construct_async(_aero: &Aerosol) -> Result<Self, Self::Error> {
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

    let aero = Aerosol::new();
    aero.init::<Arc<UserManagementImpl>>();
    aero.init_async::<Arc<Presentations>>().await;
    println!("Started...");
    Ok(())
}
