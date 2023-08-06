CREATE INDEX group_visibility_id_idx ON "group" (visibility, id);

CREATE FUNCTION visible_game_proposals(
    user_id BIGINT
) RETURNS SETOF game_proposal AS $$
    WITH shared_game_proposal_ids AS (
        SELECT DISTINCT game_proposal_id FROM request
        LEFT JOIN group_member ON request.to_group_id = group_member.group_id
        WHERE request.type_ = 'GameProposal' AND (
            request.to_user_id = user_id OR group_member.member_id = user_id
        ) AND request.game_proposal_id IS NOT NULL
        UNION
        SELECT DISTINCT game_proposal_id FROM session
        WHERE session.user_id = user_id AND session.game_proposal_id IS NOT NULL
    )
    SELECT * FROM game_proposal
    WHERE game_proposal.is_public OR game_proposal.id IN (SELECT game_proposal_id FROM shared_game_proposal_ids)
$$ LANGUAGE SQL STABLE;

CREATE FUNCTION visible_groups(
    user_id BIGINT
) RETURNS SETOF "group" AS $$
    WITH my_group_ids AS (
        SELECT DISTINCT group_id FROM group_member
        WHERE group_member.member_id = user_id
    ), friend_group_ids AS (
        SELECT DISTINCT group_id FROM user_friend
        INNER JOIN group_member ON user_friend.friend_id = group_member.member_id
        WHERE user_friend.user_id = user_id
        UNION
        SELECT group_id FROM my_group_ids
    )
    SELECT * FROM "group" WHERE "group".visibility = 'Public'
    UNION ALL
    SELECT * FROM "group" WHERE "group".visibility = 'Friends' AND "group".id IN (SELECT group_id FROM friend_group_ids)
    UNION ALL
    SELECT * FROM "group" WHERE "group".visibility = 'Private' AND "group".id IN (SELECT group_id FROM my_group_ids)
$$ LANGUAGE SQL STABLE;
