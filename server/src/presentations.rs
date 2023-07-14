use std::{any::Any, sync::Arc};

use aerosol::{Aerosol, AsyncConstructible};
use async_trait::async_trait;
use playferrous_presentation::{Presentation, UserManagement};
use playferrous_presentation_ssh::PresentationSsh;
use serde::{Deserialize, Serialize};

use crate::Config;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AnyPresentationConfig {
    Ssh(<PresentationSsh as Presentation>::Config),
}

impl AnyPresentationConfig {
    async fn start_presentation(
        &self,
        user_management: Arc<dyn UserManagement>,
    ) -> anyhow::Result<Arc<dyn Any + Send + Sync>> {
        Ok(match self {
            Self::Ssh(config) => Arc::new(PresentationSsh::new(config, user_management).await?),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Presentations {
    _presentations: Vec<Arc<dyn Any + Send + Sync>>,
}

#[async_trait]
impl AsyncConstructible for Presentations {
    type Error = anyhow::Error;
    async fn construct_async(aero: &Aerosol) -> Result<Self, Self::Error> {
        let config: Arc<Config> = aero.obtain_async().await;
        let mut presentations = Vec::new();
        for item in &config.presentation {
            presentations.push(item.start_presentation(aero.get()).await?);
        }
        Ok(Self {
            _presentations: presentations,
        })
    }
}
