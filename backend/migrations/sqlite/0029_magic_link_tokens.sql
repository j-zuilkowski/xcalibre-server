CREATE TABLE IF NOT EXISTS magic_link_tokens (
    id          TEXT    PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id     TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT    NOT NULL UNIQUE,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at  INTEGER NOT NULL,
    used_at     INTEGER
);

CREATE INDEX IF NOT EXISTS idx_magic_link_tokens_user_id   ON magic_link_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_magic_link_tokens_expires_at ON magic_link_tokens(expires_at);
