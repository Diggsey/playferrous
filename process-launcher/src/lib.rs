use std::{env::consts::EXE_SUFFIX, fmt::Debug, path::Path, process::Stdio, sync::Arc};

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use ijson::IValue;
use playferrous_launcher::{
    CommandResponse, ConsoleUi, Game, GameError, GameInstance, GameSetup, GameState, GameTick,
    GenericGame, Launcher, LauncherConfig, LauncherError,
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    process::{Child, ChildStdin, ChildStdout, Command},
};

#[derive(Serialize, Deserialize)]
pub struct ProcessLauncherConfig {
    path: String,
}

pub struct ProcessLauncher {
    config: ProcessLauncherConfig,
}

#[async_trait]
impl LauncherConfig for ProcessLauncherConfig {
    async fn start_launcher(self) -> anyhow::Result<Arc<dyn Launcher>> {
        Ok(Arc::new(ProcessLauncher { config: self }))
    }
}

#[async_trait]
impl Launcher for ProcessLauncher {
    async fn launch(&self, game_setup: GameSetup) -> Result<Box<dyn GameInstance>, LauncherError> {
        let binary_name = format!("{}{}", game_setup.game_type, EXE_SUFFIX);
        let process_path = Path::new(&self.config.path).join(binary_name);
        if !process_path.is_file() {
            return Err(LauncherError::UnknownGameType);
        }

        let mut child = Command::new(process_path)
            .arg("--playferrous")
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("Failed to spawn game process")?;

        let stdin = BufWriter::new(
            child
                .stdin
                .take()
                .context("Failed to obtain stdin for child process")?,
        );
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .context("Failed to obtain stdout for child process")?,
        );
        let mut res = Box::new(GameInstanceProcess {
            _child: child,
            stdin,
            stdout,
        });

        let req = GameProcessRequest::Initialize(game_setup);
        let resp = res.request(&req).await?;
        if !matches!(resp, GameProcessResponse::Initialize) {
            return Err(GameInstanceProcess::response_type_error(&req, &resp).into());
        }

        Ok(res)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(bound = "G: Game")]
pub enum GameProcessRequest<G: Game = GenericGame> {
    Initialize(GameSetup<G>),
    LoadSnapshot {
        tick: GameTick,
        snapshot: G::Snapshot,
    },
    SaveSnapshot,
    Advance {
        tick: GameTick,
        action: G::Action,
    },
    State,
    RenderConsoleUi {
        player: i32,
    },
    InterpretConsoleCommand {
        player: i32,
        command: String,
    },
}

impl<G: Game> Debug for GameProcessRequest<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initialize(arg0) => f.debug_tuple("Initialize").field(arg0).finish(),
            Self::LoadSnapshot { tick, snapshot } => f
                .debug_struct("LoadSnapshot")
                .field("tick", tick)
                .field("snapshot", snapshot)
                .finish(),
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

impl<G: Game> Clone for GameProcessRequest<G> {
    fn clone(&self) -> Self {
        match self {
            Self::Initialize(arg0) => Self::Initialize(arg0.clone()),
            Self::LoadSnapshot { tick, snapshot } => Self::LoadSnapshot {
                tick: tick.clone(),
                snapshot: snapshot.clone(),
            },
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
pub enum GameProcessResponse<G: Game = GenericGame> {
    Initialize,
    LoadSnapshot,
    SaveSnapshot(G::Snapshot),
    Advance,
    State(GameState),
    RenderConsoleUi(Option<ConsoleUi>),
    InterpretConsoleCommand(Option<CommandResponse<ConsoleUi, G>>),
}

impl<G: Game> Debug for GameProcessResponse<G> {
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
impl<G: Game> Clone for GameProcessResponse<G> {
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
struct GameInstanceProcess {
    _child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl GameInstanceProcess {
    async fn request(
        &mut self,
        request: &GameProcessRequest,
    ) -> anyhow::Result<GameProcessResponse> {
        // Request
        let mut request_str = serde_json::to_string(&request)?;
        request_str.push('\n');
        self.stdin.write_all(request_str.as_bytes()).await?;
        drop(request_str);
        self.stdin.flush().await?;

        // Response
        let mut buf = String::new();
        self.stdout.read_line(&mut buf).await?;
        Ok(serde_json::from_str(&buf)?)
    }
    fn response_type_error(req: &GameProcessRequest, resp: &GameProcessResponse) -> anyhow::Error {
        anyhow!("Invalid response {resp:?} for {req:?}")
    }
}

#[async_trait]
impl GameInstance for GameInstanceProcess {
    async fn load_snapshot(&mut self, tick: GameTick, snapshot: IValue) -> anyhow::Result<()> {
        let req = GameProcessRequest::LoadSnapshot { tick, snapshot };
        let resp = self.request(&req).await?;
        if let GameProcessResponse::LoadSnapshot = resp {
            Ok(())
        } else {
            Err(Self::response_type_error(&req, &resp))
        }
    }
    async fn save_snapshot(&mut self) -> anyhow::Result<IValue> {
        let req = GameProcessRequest::SaveSnapshot;
        let resp = self.request(&req).await?;
        if let GameProcessResponse::SaveSnapshot(snapshot) = resp {
            Ok(snapshot)
        } else {
            Err(Self::response_type_error(&req, &resp))
        }
    }
    async fn advance(&mut self, tick: GameTick, action: IValue) -> anyhow::Result<()> {
        let req = GameProcessRequest::Advance { tick, action };
        let resp = self.request(&req).await?;
        if let GameProcessResponse::Advance = resp {
            Ok(())
        } else {
            Err(Self::response_type_error(&req, &resp))
        }
    }
    async fn state(&mut self) -> anyhow::Result<GameState> {
        let req = GameProcessRequest::State;
        let resp = self.request(&req).await?;
        if let GameProcessResponse::State(state) = resp {
            Ok(state)
        } else {
            Err(Self::response_type_error(&req, &resp))
        }
    }

    // Presentation-specific functionality
    async fn render_console_ui(&mut self, player: i32) -> Result<ConsoleUi, GameError> {
        let req = GameProcessRequest::RenderConsoleUi { player };
        let resp = self.request(&req).await?;
        if let GameProcessResponse::RenderConsoleUi(ui) = resp {
            ui.ok_or(GameError::UnsupportedPresentationMode)
        } else {
            Err(Self::response_type_error(&req, &resp).into())
        }
    }
    async fn interpret_console_command(
        &mut self,
        player: i32,
        command: &str,
    ) -> Result<CommandResponse<ConsoleUi>, GameError> {
        let req = GameProcessRequest::InterpretConsoleCommand {
            player,
            command: command.into(),
        };
        let resp = self.request(&req).await?;
        if let GameProcessResponse::InterpretConsoleCommand(r) = resp {
            r.ok_or(GameError::UnsupportedPresentationMode)
        } else {
            Err(Self::response_type_error(&req, &resp).into())
        }
    }
}
