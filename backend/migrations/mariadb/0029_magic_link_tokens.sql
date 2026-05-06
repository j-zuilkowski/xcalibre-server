CREATE TABLE IF NOT EXISTS magic_link_tokens (
    id          VARCHAR(32)  NOT NULL PRIMARY KEY DEFAULT (LOWER(HEX(RANDOM_BYTES(16)))),
    user_id     VARCHAR(36)  NOT NULL,
    token_hash  VARCHAR(64)  NOT NULL UNIQUE,
    created_at  BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
    expires_at  BIGINT       NOT NULL,
    used_at     BIGINT,
    CONSTRAINT fk_mlt_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_magic_link_tokens_user_id    ON magic_link_tokens(user_id);
CREATE INDEX idx_magic_link_tokens_expires_at ON magic_link_tokens(expires_at);
