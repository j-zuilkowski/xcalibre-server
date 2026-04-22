CREATE TABLE oauth_accounts (
    id                TEXT PRIMARY KEY,
    user_id           TEXT NOT NULL,
    provider          TEXT NOT NULL,
    provider_user_id  TEXT NOT NULL,
    email             TEXT NOT NULL,
    created_at        DATETIME NOT NULL,
    UNIQUE KEY uq_oauth_accounts_provider_user (provider, provider_user_id),
    CONSTRAINT fk_oauth_accounts_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_oauth_accounts_user_id ON oauth_accounts(user_id);
