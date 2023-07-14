use chrono::{DateTime, Utc};
use playferrous_presentation::{GameId, GameProposalId, SessionId, UserId};

use super::PgTransaction;

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

pub async fn list_for_user(tx: &mut PgTransaction, user_id: UserId) -> sqlx::Result<Vec<Session>> {
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
        WHERE user_id = $1
        ORDER BY created_at DESC
        "#,
        user_id as _
    )
    .fetch_all(tx)
    .await?)
}

pub async fn get_by_id(
    tx: &mut PgTransaction,
    session_id: SessionId,
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
        WHERE id = $1
        "#,
        session_id as _
    )
    .fetch_optional(tx)
    .await?)
}
