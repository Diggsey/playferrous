use chrono::{DateTime, Utc};
use ijson::IValue;
use playferrous_presentation::{GameProposalId, UserId};
use sqlx::types::Json;

use super::PgTransaction;

#[derive(Debug)]
pub struct GameProposal {
    pub id: GameProposalId,
    pub game_type: String,
    pub is_public: bool,
    pub min_players: i32,
    pub max_players: i32,
    pub mod_players: i32,
    pub rules: Json<IValue>,
    pub created_at: DateTime<Utc>,
    pub deadline: DateTime<Utc>,
}

pub async fn create(
    tx: &mut PgTransaction,
    game_type: &str,
    user_id: UserId,
) -> sqlx::Result<GameProposal> {
    let proposal = sqlx::query_as!(
        GameProposal,
        r#"
        INSERT INTO game_proposal (
            game_type,
            is_public,
            min_players,
            max_players,
            mod_players,
            rules,
            deadline
        ) VALUES (
            $1,
            FALSE,
            2,
            8,
            1,
            'null'::jsonb,
            NOW() + INTERVAL '5 minutes'
        )
        RETURNING
            id as "id: _",
            game_type,
            is_public,
            min_players,
            max_players,
            mod_players,
            rules as "rules: _",
            created_at,
            deadline
        "#,
        game_type
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        r#"
            INSERT INTO session (
                type,
                user_id,
                game_proposal_id,
                is_ready
            ) VALUES (
                'GameProposal',
                $1,
                $2,
                FALSE
            )
        "#,
        user_id as _,
        proposal.id as _
    )
    .execute(&mut *tx)
    .await?;
    Ok(proposal)
}
