use chrono::{DateTime, Utc};
use playferrous_presentation::{
    GameId, GameProposalId, SessionId, SessionKind, SessionMin, UserId,
};

use super::transaction::Transaction;

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "session_type")]
pub enum SessionType {
    Game,
    GameProposal,
}

#[derive(Debug)]
pub struct Session {
    pub id: SessionId,
    pub type_: SessionType,
    pub user_id: UserId,
    pub created_at: DateTime<Utc>,
    pub game_id: Option<GameId>,
    pub game_player_index: Option<i32>,
    pub game_proposal_id: Option<GameProposalId>,
}

struct SessionMinRecord {
    pub id: SessionId,
    pub type_: SessionType,
    pub created_at: DateTime<Utc>,
    pub game_id: Option<GameId>,
    pub game_proposal_id: Option<GameProposalId>,
}

impl SessionMinRecord {
    pub fn reify(self) -> SessionMin {
        SessionMin {
            id: self.id,
            created_at: self.created_at,
            kind: match self.type_ {
                SessionType::Game => SessionKind::Game(self.game_id.unwrap()),
                SessionType::GameProposal => {
                    SessionKind::GameProposal(self.game_proposal_id.unwrap())
                }
            },
        }
    }
}

pub async fn list_for_user(tx: &mut Transaction, user_id: UserId) -> sqlx::Result<Vec<SessionMin>> {
    let records = sqlx::query_as!(
        SessionMinRecord,
        r#"
        SELECT
            id as "id: _",
            "type" as "type_: _",
            created_at,
            game_id as "game_id: _",
            game_proposal_id as "game_proposal_id: _"
        FROM session
        WHERE user_id = $1
        ORDER BY created_at DESC
        "#,
        user_id as _
    )
    .fetch_all(tx)
    .await?;
    Ok(records.into_iter().map(|r| r.reify()).collect())
}

pub async fn get_by_id_and_user(
    tx: &mut Transaction,
    session_id: SessionId,
    user_id: UserId,
) -> sqlx::Result<Option<Session>> {
    Ok(sqlx::query_as!(
        Session,
        r#"
        SELECT
            id as "id: _",
            "type" as "type_: _",
            user_id as "user_id: _",
            created_at,
            game_id as "game_id: _",
            game_player_index,
            game_proposal_id as "game_proposal_id: _"
        FROM session
        WHERE id = $1 AND user_id = $2
        "#,
        session_id as _,
        user_id as _
    )
    .fetch_optional(tx)
    .await?)
}
