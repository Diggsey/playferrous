use std::sync::Arc;

use aerosol::{Aerosol, AsyncConstructible};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use playferrous_launcher::{Launcher, LauncherConfig};
use playferrous_process_launcher::ProcessLauncherConfig;

use crate::Config;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AnyLauncherConfig {
    Process(ProcessLauncherConfig),
}

#[async_trait]
impl LauncherConfig for AnyLauncherConfig {
    async fn start_launcher(&self) -> anyhow::Result<Arc<dyn Launcher>> {
        match self {
            AnyLauncherConfig::Process(c) => c.start_launcher().await,
        }
    }
}

#[derive(Debug)]
pub struct Launchers {
    launchers: Vec<Arc<dyn Launcher>>,
}

#[async_trait]
impl AsyncConstructible for Launchers {
    type Error = anyhow::Error;
    async fn construct_async(aero: &Aerosol) -> Result<Self, Self::Error> {
        let config: Arc<Config> = aero.obtain_async().await;
        let mut launchers = Vec::new();
        for item in &config.launcher {
            launchers.push(item.start_launcher().await?);
        }
        Ok(Self { launchers })
    }
}
