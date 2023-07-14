
CREATE TYPE session_type AS ENUM ('Game', 'GameProposal');

CREATE TABLE session (
    id BIGSERIAL PRIMARY KEY,
    "type" session_type NOT NULL,
    user_id BIGINT NOT NULL REFERENCES "user" ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Only populated when "type" == 'Game'
    game_id BIGINT REFERENCES "game" ON DELETE CASCADE,
    game_player_index INT,
    -- Only populated when "type" == 'GameProposal'
    game_proposal_id BIGINT REFERENCES "game_proposal" ON DELETE CASCADE,
    is_ready BOOLEAN,
    UNIQUE (game_id, game_player_index),
    UNIQUE (user_id, game_proposal_id),
    CHECK (CASE "type"
        WHEN 'Game' THEN game_id IS NOT NULL AND game_player_index IS NOT NULL AND game_proposal_id IS NULL AND is_ready IS NULL
        WHEN 'GameProposal' THEN game_id IS NULL AND game_player_index IS NULL AND game_proposal_id IS NOT NULL AND is_ready IS NOT NULL
    END)
);
CREATE INDEX ON session(user_id, created_at);

DROP TABLE game_proposal_acceptee;
