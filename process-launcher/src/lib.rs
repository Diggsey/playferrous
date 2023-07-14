use std::{env::consts::EXE_SUFFIX, path::Path, process::Stdio, sync::Arc};

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use ijson::IValue;
use playferrous_launcher::{GameError, GameInstance, Launcher, LauncherConfig, LauncherError};
use playferrous_types::{
    CommandResponse, ConsoleUi, GameRequest, GameResponse, GameSetup, GameState, GameTick,
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    process::{Child, ChildStdin, ChildStdout, Command},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessLauncherConfig {
    path: String,
}

#[derive(Debug)]
pub struct ProcessLauncher {
    config: ProcessLauncherConfig,
}

#[async_trait]
impl LauncherConfig for ProcessLauncherConfig {
    async fn start_launcher(&self) -> anyhow::Result<Arc<dyn Launcher>> {
        Ok(Arc::new(ProcessLauncher {
            config: self.clone(),
        }))
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

        let req = GameRequest::Initialize(game_setup);
        let resp = res.request(&req).await?;
        if !matches!(resp, GameResponse::Initialize) {
            return Err(GameInstanceProcess::response_type_error(&req, &resp).into());
        }

        Ok(res)
    }
}

struct GameInstanceProcess {
    _child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl GameInstanceProcess {
    async fn request(&mut self, request: &GameRequest) -> anyhow::Result<GameResponse> {
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
    fn response_type_error(req: &GameRequest, resp: &GameResponse) -> anyhow::Error {
        anyhow!("Invalid response {resp:?} for {req:?}")
    }
}

#[async_trait]
impl GameInstance for GameInstanceProcess {
    async fn load_snapshot(&mut self, snapshot: IValue) -> anyhow::Result<()> {
        let req = GameRequest::LoadSnapshot(snapshot);
        let resp = self.request(&req).await?;
        if let GameResponse::LoadSnapshot = resp {
            Ok(())
        } else {
            Err(Self::response_type_error(&req, &resp))
        }
    }
    async fn save_snapshot(&mut self) -> anyhow::Result<IValue> {
        let req = GameRequest::SaveSnapshot;
        let resp = self.request(&req).await?;
        if let GameResponse::SaveSnapshot(snapshot) = resp {
            Ok(snapshot)
        } else {
            Err(Self::response_type_error(&req, &resp))
        }
    }
    async fn advance(&mut self, tick: GameTick, action: IValue) -> anyhow::Result<()> {
        let req = GameRequest::Advance { tick, action };
        let resp = self.request(&req).await?;
        if let GameResponse::Advance = resp {
            Ok(())
        } else {
            Err(Self::response_type_error(&req, &resp))
        }
    }
    async fn state(&mut self) -> anyhow::Result<GameState> {
        let req = GameRequest::State;
        let resp = self.request(&req).await?;
        if let GameResponse::State(state) = resp {
            Ok(state)
        } else {
            Err(Self::response_type_error(&req, &resp))
        }
    }

    // Presentation-specific functionality
    async fn render_console_ui(&mut self, player: i32) -> Result<ConsoleUi, GameError> {
        let req = GameRequest::RenderConsoleUi { player };
        let resp = self.request(&req).await?;
        if let GameResponse::RenderConsoleUi(ui) = resp {
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
        let req = GameRequest::InterpretConsoleCommand {
            player,
            command: command.into(),
        };
        let resp = self.request(&req).await?;
        if let GameResponse::InterpretConsoleCommand(r) = resp {
            r.ok_or(GameError::UnsupportedPresentationMode)
        } else {
            Err(Self::response_type_error(&req, &resp).into())
        }
    }
}
