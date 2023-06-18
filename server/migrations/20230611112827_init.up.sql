CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE "user" (
    id BIGSERIAL PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    password_salt TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE user_key (
    user_id BIGINT NOT NULL,
    fingerprint TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY(user_id, fingerprint)
);

CREATE TABLE user_friend (
    user_id BIGINT NOT NULL REFERENCES "user" ON DELETE CASCADE,
    friend_id BIGINT NOT NULL REFERENCES "user" ON DELETE CASCADE,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, friend_id)
);

CREATE TYPE group_visibility AS ENUM ('Public', 'Friends', 'Private');

CREATE TABLE "group" (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    visibility group_visibility NOT NULL,
    can_directly_join BOOLEAN NOT NULL,
    can_be_invited BOOLEAN NOT NULL,
    can_request_Join BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TYPE membership_type AS ENUM ('Regular', 'Admin');

CREATE TABLE group_member (
    group_id BIGINT NOT NULL REFERENCES "group" ON DELETE CASCADE,
    member_id BIGINT NOT NULL REFERENCES "user" ON DELETE CASCADE,
    membership_type membership_type NOT NULL,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (group_id, member_id)
);

CREATE INDEX ON group_member(member_id, membership_type);

CREATE TABLE game_proposal (
    id BIGSERIAL PRIMARY KEY,
    game_type TEXT NOT NULL,
    is_public BOOLEAN NOT NULL,
    min_players INT NOT NULL,
    max_players INT NOT NULL,
    mod_players INT NOT NULL,
    rules JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deadline TIMESTAMPTZ NOT NULL
);

CREATE TABLE game_proposal_acceptee (
    game_proposal_id BIGINT NOT NULL REFERENCES game_proposal,
    acceptee_id BIGINT NOT NULL REFERENCES "user" ON DELETE CASCADE,
    accepted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_ready BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (game_proposal_id, acceptee_id)
);

CREATE INDEX ON game_proposal_acceptee(acceptee_id, accepted_at);

CREATE TABLE game (
    id BIGSERIAL PRIMARY KEY,
    game_type TEXT NOT NULL,
    is_public BOOLEAN NOT NULL,
    num_players INT NOT NULL,
    rules JSONB NOT NULL,
    seed BIGINT NOT NULL,
    snapshot JSONB NOT NULL,
    snapshot_ply INT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

CREATE TABLE game_player (
    game_id BIGINT NOT NULL REFERENCES game,
    player_index INT NOT NULL,
    initial_player_id BIGINT REFERENCES "user" ON DELETE SET NULL,
    player_id BIGINT REFERENCES "user" ON DELETE SET NULL,
    result_position INT,
    result_score BIGINT,
    PRIMARY KEY (game_id, player_index)
);

CREATE INDEX ON game_player(player_id);
CREATE INDEX ON game_player(initial_player_id);

CREATE TABLE game_step (
    id BIGINT NOT NULL REFERENCES game,
    ply INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    data JSONB,
    PRIMARY KEY (id, ply)
);

CREATE TYPE request_type AS ENUM ('Friend', 'JoinGroup', 'GroupInvite', 'GameProposal', 'GameInvite');

CREATE TABLE request (
    id BIGSERIAL PRIMARY KEY,
    type_ request_type NOT NULL,
    from_user_id BIGINT NOT NULL REFERENCES "user" ON DELETE CASCADE,
    from_group_id BIGINT REFERENCES "group" ON DELETE CASCADE,
    to_user_id BIGINT REFERENCES "user" ON DELETE CASCADE,
    to_group_id BIGINT REFERENCES "group" ON DELETE CASCADE,
    game_proposal_id BIGINT REFERENCES game_proposal ON DELETE CASCADE,
    game_id BIGINT,
    player_index INT,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (type_, from_user_id, from_group_id, to_user_id, to_group_id),
    FOREIGN KEY (game_id, player_index) REFERENCES game_player
);

CREATE TABLE message (
    id BIGSERIAL PRIMARY KEY,
    to_id BIGINT NOT NULL REFERENCES "user" ON DELETE CASCADE,
    from_id BIGINT REFERENCES "user" ON DELETE SET NULL,
    subject TEXT NOT NULL,
    body TEXT NOT NULL,
    was_read BOOLEAN NOT NULL DEFAULT FALSE,
    request_id BIGINT REFERENCES request ON DELETE SET NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX ON message(to_id, sent_at);
