use std::{
    fmt::Debug,
    ops::{Add, AddAssign, Mul, MulAssign, Sub, SubAssign},
};

use ijson::IValue;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[cfg(feature = "process")]
pub mod process;

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

#[derive(Serialize, Deserialize)]
#[serde(bound = "T: GameUi, G: Game")]
pub struct CommandResponse<T: GameUi, G: Game = GenericGame> {
    pub update_ui: Option<T>,
    pub advance: Option<G::Action>,
}

impl<T: GameUi, G: Game> CommandResponse<T, G> {
    pub const IGNORE: Self = Self {
        update_ui: None,
        advance: None,
    };
}

impl<T: GameUi, G: Game> Debug for CommandResponse<T, G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandResponse")
            .field("update_ui", &self.update_ui)
            .field("advance", &self.advance)
            .finish()
    }
}

impl<T: GameUi, G: Game> Clone for CommandResponse<T, G> {
    fn clone(&self) -> Self {
        Self {
            update_ui: self.update_ui.clone(),
            advance: self.advance.clone(),
        }
    }
}

impl<T: GameUi, G: Game> Default for CommandResponse<T, G> {
    fn default() -> Self {
        Self::IGNORE
    }
}

#[derive(
    Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Default,
)]
pub struct GameTick(pub i64);

impl Add for GameTick {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for GameTick {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Mul<i64> for GameTick {
    type Output = GameTick;

    fn mul(self, rhs: i64) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl Mul<GameTick> for i64 {
    type Output = GameTick;

    fn mul(self, rhs: GameTick) -> Self::Output {
        GameTick(self * rhs.0)
    }
}

impl AddAssign for GameTick {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for GameTick {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl MulAssign<i64> for GameTick {
    fn mul_assign(&mut self, rhs: i64) {
        self.0 *= rhs;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct InProgressGameState {
    pub player_turn: i32,
    pub deadline: GameTick,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerResult {
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

#[derive(Serialize, Deserialize)]
#[serde(bound = "G: Game")]
pub enum GameRequest<G: Game = GenericGame> {
    Initialize(GameSetup<G>),
    LoadSnapshot(G::Snapshot),
    SaveSnapshot,
    Advance { tick: GameTick, action: G::Action },
    State,
    RenderConsoleUi { player: i32 },
    InterpretConsoleCommand { player: i32, command: String },
}

impl<G: Game> Debug for GameRequest<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initialize(arg0) => f.debug_tuple("Initialize").field(arg0).finish(),
            Self::LoadSnapshot(snapshot) => f.debug_tuple("LoadSnapshot").field(snapshot).finish(),
            Self::SaveSnapshot => write!(f, "SaveSnapshot"),
            Self::Advance { tick, action } => f
                .debug_struct("Advance")
                .field("tick", tick)
                .field("action", action)
                .finish(),
            Self::State => write!(f, "State"),
            Self::RenderConsoleUi { player } => f
                .debug_struct("RenderConsoleUi")
                .field("player", player)
                .finish(),
            Self::InterpretConsoleCommand { player, command } => f
                .debug_struct("InterpretConsoleCommand")
                .field("player", player)
                .field("command", command)
                .finish(),
        }
    }
}

impl<G: Game> Clone for GameRequest<G> {
    fn clone(&self) -> Self {
        match self {
            Self::Initialize(arg0) => Self::Initialize(arg0.clone()),
            Self::LoadSnapshot(snapshot) => Self::LoadSnapshot(snapshot.clone()),
            Self::SaveSnapshot => Self::SaveSnapshot,
            Self::Advance { tick, action } => Self::Advance {
                tick: tick.clone(),
                action: action.clone(),
            },
            Self::State => Self::State,
            Self::RenderConsoleUi { player } => Self::RenderConsoleUi {
                player: player.clone(),
            },
            Self::InterpretConsoleCommand { player, command } => Self::InterpretConsoleCommand {
                player: player.clone(),
                command: command.clone(),
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(bound = "G: Game")]
pub enum GameResponse<G: Game = GenericGame> {
    Initialize,
    LoadSnapshot,
    SaveSnapshot(G::Snapshot),
    Advance,
    State(GameState),
    RenderConsoleUi(Option<ConsoleUi>),
    InterpretConsoleCommand(Option<CommandResponse<ConsoleUi, G>>),
}

impl<G: Game> Debug for GameResponse<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initialize => write!(f, "Initialize"),
            Self::LoadSnapshot => write!(f, "LoadSnapshot"),
            Self::SaveSnapshot(arg0) => f.debug_tuple("SaveSnapshot").field(arg0).finish(),
            Self::Advance => write!(f, "Advance"),
            Self::State(arg0) => f.debug_tuple("State").field(arg0).finish(),
            Self::RenderConsoleUi(arg0) => f.debug_tuple("RenderConsoleUi").field(arg0).finish(),
            Self::InterpretConsoleCommand(arg0) => f
                .debug_tuple("InterpretConsoleCommand")
                .field(arg0)
                .finish(),
        }
    }
}
impl<G: Game> Clone for GameResponse<G> {
    fn clone(&self) -> Self {
        match self {
            Self::Initialize => Self::Initialize,
            Self::LoadSnapshot => Self::LoadSnapshot,
            Self::SaveSnapshot(arg0) => Self::SaveSnapshot(arg0.clone()),
            Self::Advance => Self::Advance,
            Self::State(arg0) => Self::State(arg0.clone()),
            Self::RenderConsoleUi(arg0) => Self::RenderConsoleUi(arg0.clone()),
            Self::InterpretConsoleCommand(arg0) => Self::InterpretConsoleCommand(arg0.clone()),
        }
    }
}
