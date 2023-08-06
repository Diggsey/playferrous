use std::{convert::Infallible, sync::Arc};

use aerosol::{Aero, Constructible};
use async_trait::async_trait;
use dashmap::DashMap;
use playferrous_presentation::{
    actor::Actor,
    bichannel::{bichannel, Bichannel},
    GameId, PresentationKind,
};
use tokio::sync::mpsc;

use crate::connection_manager::{ConnectionToSessionMsg, SessionToConnectionMsg};

#[derive(Debug)]
struct EnterGameSession {
    player_index: i32,
    bichannel: Bichannel<SessionToConnectionMsg, ConnectionToSessionMsg>,
    kind: PresentationKind,
}

#[derive(Debug)]
enum SystemToGameMsg {
    Enter(EnterGameSession),
}

#[derive(Debug)]
struct Game {
    s: mpsc::Sender<SystemToGameMsg>,
}

#[derive(Debug, Clone)]
pub struct GameManager {
    games: Arc<DashMap<GameId, Game>>,
    aero: Aero,
}

impl Constructible for GameManager {
    type Error = Infallible;
    fn construct(aero: &Aero) -> Result<Self, Self::Error> {
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
        kind: PresentationKind,
    ) -> anyhow::Result<Bichannel<ConnectionToSessionMsg, SessionToConnectionMsg>> {
        let s = {
            self.games
                .entry(game_id)
                .or_insert_with(|| self.start_game(game_id))
                .s
                .clone()
        };
        let (session_bichannel, connection_bichannel) = bichannel(4);
        s.send(SystemToGameMsg::Enter(EnterGameSession {
            player_index,
            bichannel: connection_bichannel,
            kind,
        }))
        .await?;
        Ok(session_bichannel)
    }

    fn start_game(&self, game_id: GameId) -> Game {
        let (system_s, system_r) = mpsc::channel(4);
        GameActor {
            aero: self.aero.clone(),
            game_id,
            system_r,
        }
        .spawn();
        Game { s: system_s }
    }
}

struct GameActor {
    aero: Aero,
    game_id: GameId,
    system_r: mpsc::Receiver<SystemToGameMsg>,
}

#[async_trait]
impl Actor for GameActor {
    async fn run(self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl Drop for GameActor {
    fn drop(&mut self) {
        self.aero
            .obtain::<GameManager>()
            .games
            .remove(&self.game_id);
    }
}
