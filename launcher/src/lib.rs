use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;
use ijson::IValue;
use playferrous_types::{CommandResponse, ConsoleUi, GameSetup, GameState, GameTick};
use serde::de::DeserializeOwned;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LauncherError {
    #[error("Unknown game type")]
    UnknownGameType,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum GameError {
    #[error("Unsupported presentation mode")]
    UnsupportedPresentationMode,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[async_trait]
pub trait Launcher: Send + Sync + Debug {
    async fn launch(&self, game_setup: GameSetup) -> Result<Box<dyn GameInstance>, LauncherError>;
}

#[async_trait]
pub trait LauncherConfig: DeserializeOwned {
    async fn start_launcher(&self) -> anyhow::Result<Arc<dyn Launcher>>;
}

#[async_trait]
pub trait GameInstance: Send + Sync {
    async fn load_snapshot(&mut self, snapshot: IValue) -> anyhow::Result<()>;
    async fn save_snapshot(&mut self) -> anyhow::Result<IValue>;
    async fn advance(&mut self, tick: GameTick, action: IValue) -> anyhow::Result<()>;
    async fn state(&mut self) -> anyhow::Result<GameState>;

    // Presentation-specific functionality
    async fn render_console_ui(&mut self, _player: i32) -> Result<ConsoleUi, GameError> {
        Err(GameError::UnsupportedPresentationMode)
    }
    async fn interpret_console_command(
        &mut self,
        _player: i32,
        _command: &str,
    ) -> Result<CommandResponse<ConsoleUi>, GameError> {
        Err(GameError::UnsupportedPresentationMode)
    }
}
