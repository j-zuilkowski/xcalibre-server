CREATE TABLE webhooks (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    url             TEXT NOT NULL,
    secret          TEXT NOT NULL,
    events          TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    last_delivery_at TEXT,
    last_error      TEXT,
    created_at      TEXT NOT NULL
);

CREATE INDEX idx_webhooks_user ON webhooks(user_id);

CREATE TABLE webhook_deliveries (
    id              TEXT PRIMARY KEY,
    webhook_id      TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event           TEXT NOT NULL,
    payload         TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK(status IN ('pending', 'delivered', 'failed')),
    attempts        INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT,
    response_status INTEGER,
    created_at      TEXT NOT NULL,
    delivered_at    TEXT
);

CREATE INDEX idx_webhook_deliveries_pending ON webhook_deliveries(status, next_attempt_at);
