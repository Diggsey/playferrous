use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{Arc, Mutex},
};

use aerosol::{Aerosol, Constructible};
use playferrous_presentation::GameId;
use tokio::sync::mpsc;

use crate::active_session::SessionLink;

#[derive(Debug)]
struct EnterGameSession {
    player_index: i32,
    link: SessionLink,
}

#[derive(Debug)]
struct GameHandle {
    tx: mpsc::Sender<EnterGameSession>,
}

#[derive(Debug)]
pub struct GameManager {
    games: Arc<Mutex<HashMap<GameId, GameHandle>>>,
    aero: Aerosol,
}

impl Constructible for GameManager {
    type Error = Infallible;
    fn construct(aero: &Aerosol) -> Result<Self, Self::Error> {
        Ok(Self {
            games: Default::default(),
            aero: aero.clone(),
        })
    }
}

impl GameManager {
    pub async fn enter_session(
        &self,
        game_id: GameId,
        player_index: i32,
        link: SessionLink,
    ) -> anyhow::Result<()> {
        let tx = {
            let mut guard = self.games.lock().expect("Lock to not be poisoned...");
            guard
                .entry(game_id)
                .or_insert_with(|| self.start_game(game_id))
                .tx
                .clone()
        };
        tx.send(EnterGameSession { player_index, link }).await?;
        Ok(())
    }

    fn start_game(&self, game_id: GameId) -> GameHandle {
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(
            Game {
                game_id,
                rx,
                games: self.games.clone(),
            }
            .run(),
        );
        GameHandle { tx }
    }
}

struct Game {
    game_id: GameId,
    rx: mpsc::Receiver<EnterGameSession>,
    games: Arc<Mutex<HashMap<GameId, GameHandle>>>,
}

impl Game {
    async fn run(self) {}
}

impl Drop for Game {
    fn drop(&mut self) {
        self.games
            .lock()
            .expect("Mutex to not be poisoned")
            .remove(&self.game_id);
    }
}
