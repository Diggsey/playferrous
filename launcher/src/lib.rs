use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use ijson::IValue;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

pub trait Game {
    type Snapshot: Serialize + DeserializeOwned + Debug + Clone;
    type Action: Serialize + DeserializeOwned + Debug + Clone;
    type Rules: Serialize + DeserializeOwned + Debug + Clone;
}

pub struct GenericGame;

impl Game for GenericGame {
    type Snapshot = IValue;
    type Action = IValue;
    type Rules = IValue;
}

pub trait GameUi: Serialize + DeserializeOwned + Debug + Clone {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleUi {
    pub prompt: String,
}

impl GameUi for ConsoleUi {}

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

#[derive(Serialize, Deserialize)]
#[serde(bound = "T: GameUi, G: Game")]
pub enum CommandResponse<T: GameUi, G: Game = GenericGame> {
    Ignore,
    UpdateUi(T),
    Advance(G::Action),
}

impl<T: GameUi, G: Game> Debug for CommandResponse<T, G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ignore => write!(f, "Ignore"),
            Self::UpdateUi(arg0) => f.debug_tuple("UpdateUi").field(arg0).finish(),
            Self::Advance(arg0) => f.debug_tuple("Advance").field(arg0).finish(),
        }
    }
}

impl<T: GameUi, G: Game> Clone for CommandResponse<T, G> {
    fn clone(&self) -> Self {
        match self {
            Self::Ignore => Self::Ignore,
            Self::UpdateUi(arg0) => Self::UpdateUi(arg0.clone()),
            Self::Advance(arg0) => Self::Advance(arg0.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct GameTick(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct InProgressGameState {
    pub player_turn: i32,
    pub deadline: GameTick,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerResult {
    pub position: i32,
    pub score: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct GameResult {
    pub player_results: Vec<PlayerResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameState {
    InProgress(InProgressGameState),
    Complete(GameResult),
}

#[derive(Serialize, Deserialize)]
#[serde(bound = "G: Game")]
pub struct GameSetup<G: Game = GenericGame> {
    pub game_type: String,
    pub num_players: i32,
    pub seed: i64,
    pub rules: G::Rules,
}

impl<G: Game> Clone for GameSetup<G> {
    fn clone(&self) -> Self {
        Self {
            game_type: self.game_type.clone(),
            num_players: self.num_players.clone(),
            seed: self.seed.clone(),
            rules: self.rules.clone(),
        }
    }
}

impl<G: Game> Debug for GameSetup<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameSetup")
            .field("game_type", &self.game_type)
            .field("num_players", &self.num_players)
            .field("seed", &self.seed)
            .field("rules", &self.rules)
            .finish()
    }
}

#[async_trait]
pub trait Launcher: Send + Sync {
    async fn launch(&self, game_setup: GameSetup) -> Result<Box<dyn GameInstance>, LauncherError>;
}

#[async_trait]
pub trait LauncherConfig: DeserializeOwned {
    async fn start_launcher(self) -> anyhow::Result<Arc<dyn Launcher>>;
}

#[async_trait]
pub trait GameInstance: Send + Sync {
    async fn load_snapshot(&mut self, tick: GameTick, snapshot: IValue) -> anyhow::Result<()>;
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
