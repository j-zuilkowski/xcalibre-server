CREATE TABLE api_tokens (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    token_hash TEXT NOT NULL UNIQUE,
    created_by TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    last_used_at TEXT
);

CREATE INDEX idx_api_tokens_created_by ON api_tokens(created_by);
CREATE INDEX idx_api_tokens_hash ON api_tokens(token_hash);
