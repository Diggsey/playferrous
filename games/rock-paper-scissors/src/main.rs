use std::{
    fmt::{self, Display, Write},
    ops::Neg,
};

use anyhow::bail;
use playferrous_types::{
    process::GameProcess, CommandResponse, ConsoleUi, Game, GameResult, GameSetup, GameState,
    GameTick, InProgressGameState, PlayerResult,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Rules {
    num_rounds: i64,
    turn_timeout: GameTick,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
enum Action {
    Rock,
    Paper,
    Scissors,
}

impl Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Rock => "rock",
            Self::Paper => "paper",
            Self::Scissors => "scissors",
        })
    }
}

#[derive(Debug, Copy, Clone)]
enum Outcome {
    Won,
    Lost,
    Drew,
}

impl Neg for Outcome {
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            Self::Won => Self::Lost,
            Self::Lost => Self::Won,
            Self::Drew => Self::Drew,
        }
    }
}

impl Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Won => "won",
            Self::Lost => "lost",
            Self::Drew => "drew",
        })
    }
}

impl Outcome {
    fn score(self) -> i64 {
        match self {
            Self::Won => 3,
            Self::Lost => 0,
            Self::Drew => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Snapshot {
    player0_score: i64,
    player1_score: i64,
    rounds_played: i64,
    player0_action: Option<Action>,
    last_action: GameTick,
    player0_prompt: String,
    player1_prompt: String,
}

struct RockPaperScissors {
    rules: Rules,
    state: Snapshot,
}

impl RockPaperScissors {
    fn player_turn(&self) -> i32 {
        if self.state.player0_action.is_some() {
            1
        } else {
            0
        }
    }
}

impl Game for RockPaperScissors {
    type Snapshot = Snapshot;
    type Action = Option<Action>;
    type Rules = Rules;
}

impl GameProcess for RockPaperScissors {
    fn new(setup: GameSetup<Self>) -> anyhow::Result<Self> {
        Ok(Self {
            rules: setup.rules,
            state: Snapshot::default(),
        })
    }

    fn load_snapshot(&mut self, snapshot: Self::Snapshot) -> anyhow::Result<()> {
        self.state = snapshot;
        Ok(())
    }

    fn save_snapshot(&mut self) -> anyhow::Result<Self::Snapshot> {
        Ok(self.state.clone())
    }

    fn advance(&mut self, tick: GameTick, action: Self::Action) -> anyhow::Result<()> {
        if let Some(player0_action) = self.state.player0_action.take() {
            let player0_outcome = if let Some(player1_action) = action {
                let player0_outcome = match (player0_action, player1_action) {
                    (Action::Rock, Action::Rock)
                    | (Action::Paper, Action::Paper)
                    | (Action::Scissors, Action::Scissors) => Outcome::Drew,
                    (Action::Rock, Action::Paper)
                    | (Action::Paper, Action::Scissors)
                    | (Action::Scissors, Action::Rock) => Outcome::Lost,
                    (Action::Rock, Action::Scissors)
                    | (Action::Scissors, Action::Paper)
                    | (Action::Paper, Action::Rock) => Outcome::Won,
                };
                let player1_outcome = -player0_outcome;

                self.state.player0_prompt = format!(
                    "You played {player0_action} and {player0_outcome} against {player1_action}."
                );
                self.state.player1_prompt = format!(
                    "You played {player1_action} and {player1_outcome} against {player0_action}."
                );
                player0_outcome
            } else {
                self.state.player0_prompt =
                    "You won this round because the other player took too long to go.".into();
                self.state.player1_prompt =
                    "You lost this round because you took too long to go.".into();
                Outcome::Won
            };
            self.state.player0_score += player0_outcome.score();
            self.state.player1_score += (-player0_outcome).score();
            self.state.rounds_played += 1;
        } else {
            if action.is_none() {
                self.state.player0_prompt =
                    "You lost this round because you took too long to go.".into();
                self.state.player1_prompt =
                    "You won this round because the other player took too long to go.".into();
                self.state.player1_score += 3;
                self.state.rounds_played += 1;
            }
            self.state.player0_action = action;
        }
        self.state.last_action = tick;
        Ok(())
    }

    fn state(&mut self) -> anyhow::Result<GameState> {
        Ok(if self.state.rounds_played < self.rules.num_rounds {
            GameState::InProgress(InProgressGameState {
                player_turn: self.player_turn(),
                deadline: self.state.last_action + self.rules.turn_timeout,
            })
        } else {
            GameState::Complete(GameResult {
                player_results: vec![
                    PlayerResult {
                        score: self.state.player0_score,
                    },
                    PlayerResult {
                        score: self.state.player1_score,
                    },
                ],
            })
        })
    }

    fn interpret_console_command(
        &mut self,
        player: i32,
        command: &str,
    ) -> anyhow::Result<Option<CommandResponse<ConsoleUi, Self>>> {
        Ok(Some(if player != self.player_turn() {
            CommandResponse {
                update_ui: Some(ConsoleUi {
                    prompt: "It's not your turn yet!".into(),
                }),
                ..Default::default()
            }
        } else {
            let action = match command.to_ascii_lowercase().as_str() {
                "r" | "rock" => Action::Rock,
                "p" | "paper" => Action::Paper,
                "s" | "scissors" => Action::Scissors,
                other => {
                    return Ok(Some(CommandResponse {
                        update_ui: Some(ConsoleUi {
                            prompt: format!("Invalid command: {other}"),
                        }),
                        ..Default::default()
                    }))
                }
            };
            CommandResponse {
                advance: Some(Some(action)),
                ..Default::default()
            }
        }))
    }

    fn render_console_ui(&mut self, player: i32) -> anyhow::Result<Option<ConsoleUi>> {
        let mut prompt = String::new();
        if self.state.player0_action.is_none() {
            if self.state.rounds_played > 0 {
                write!(
                    prompt,
                    "Round {} of {} - ",
                    self.state.rounds_played, self.rules.num_rounds
                )?;
                match player {
                    0 => writeln!(prompt, "{}", self.state.player0_prompt)?,
                    1 => writeln!(prompt, "{}", self.state.player1_prompt)?,
                    _ => bail!("Invalid player number"),
                }
            }
        }
        if self.state.rounds_played < self.rules.num_rounds {
            write!(
                prompt,
                "Round {} of {} - ",
                self.state.rounds_played + 1,
                self.rules.num_rounds
            )?;
            if self.player_turn() == player {
                writeln!(prompt, "It's your go! Enter [r]ock, [p]aper or [s]cissors:")?;
            } else {
                writeln!(prompt, "Waiting for the other player...")?;
            }
        }

        Ok(None)
    }
}

fn main() -> anyhow::Result<()> {
    RockPaperScissors::main()
}
