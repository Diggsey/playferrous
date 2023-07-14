
CREATE TABLE game_proposal_acceptee (
    game_proposal_id BIGINT NOT NULL REFERENCES game_proposal,
    acceptee_id BIGINT NOT NULL REFERENCES "user" ON DELETE CASCADE,
    accepted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_ready BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (game_proposal_id, acceptee_id)
);

CREATE INDEX ON game_proposal_acceptee(acceptee_id, accepted_at);

DROP TABLE session;
DROP TYPE session_type;
