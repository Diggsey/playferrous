use std::io::{stdin, stdout, Write};

use anyhow::bail;

use crate::{
    CommandResponse, ConsoleUi, Game, GameRequest, GameResponse, GameSetup, GameState, GameTick,
};

pub trait GameProcess: Game + Sized {
    fn new(setup: GameSetup<Self>) -> anyhow::Result<Self>;
    fn load_snapshot(&mut self, snapshot: Self::Snapshot) -> anyhow::Result<()>;
    fn save_snapshot(&mut self) -> anyhow::Result<Self::Snapshot>;
    fn advance(&mut self, tick: GameTick, action: Self::Action) -> anyhow::Result<()>;
    fn state(&mut self) -> anyhow::Result<GameState>;

    // Presentation-specific functionality
    fn render_console_ui(&mut self, _player: i32) -> anyhow::Result<Option<ConsoleUi>> {
        Ok(None)
    }
    fn interpret_console_command(
        &mut self,
        _player: i32,
        _command: &str,
    ) -> anyhow::Result<Option<CommandResponse<ConsoleUi, Self>>> {
        Ok(None)
    }

    fn main() -> anyhow::Result<()> {
        let mut game: Option<Self> = None;
        for line in stdin().lines() {
            let line = line?;
            log::debug!("Request: {line}");
            let request: GameRequest<Self> = serde_json::from_str(&line)?;
            let response: GameResponse<Self> = match (&mut game, request) {
                (None, GameRequest::Initialize(setup)) => {
                    game = Some(Self::new(setup)?);
                    GameResponse::Initialize
                }
                (Some(game), GameRequest::LoadSnapshot(snapshot)) => {
                    game.load_snapshot(snapshot)?;
                    GameResponse::LoadSnapshot
                }
                (Some(game), GameRequest::SaveSnapshot) => {
                    GameResponse::SaveSnapshot(game.save_snapshot()?)
                }
                (Some(game), GameRequest::Advance { tick, action }) => {
                    game.advance(tick, action)?;
                    GameResponse::Advance
                }
                (Some(game), GameRequest::State) => GameResponse::State(game.state()?),
                (Some(game), GameRequest::RenderConsoleUi { player }) => {
                    GameResponse::RenderConsoleUi(game.render_console_ui(player)?)
                }
                (Some(game), GameRequest::InterpretConsoleCommand { player, command }) => {
                    GameResponse::InterpretConsoleCommand(
                        game.interpret_console_command(player, &command)?,
                    )
                }
                (_, request) => bail!("Unexpected gmae request: {request:?}"),
            };

            {
                let response_line = serde_json::to_string(&response)?;
                log::debug!("Response: {response_line}");
                let mut o = stdout().lock();
                writeln!(o, "{}", response_line)?;
                o.flush()?;
            }
        }
        Ok(())
    }
}
