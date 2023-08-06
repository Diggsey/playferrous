use chrono::{DateTime, Utc};
use playferrous_presentation::{GameId, GameProposalId, GroupId, RequestId, UserId};

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "request_type")]
pub enum RequestType {
    Friend,
    JoinGroup,
    GroupInvite,
    GameProposal,
    GameInvite,
}

#[derive(Debug)]
pub struct Request {
    pub id: RequestId,
    pub type_: RequestType,
    pub from_user_id: Option<UserId>,
    pub from_group_id: Option<GroupId>,
    pub to_user_id: Option<UserId>,
    pub to_group_id: Option<GroupId>,
    pub game_proposal_id: Option<GameProposalId>,
    pub game_id: Option<GameId>,
    pub player_index: i32,
    pub sent_at: DateTime<Utc>,
}
