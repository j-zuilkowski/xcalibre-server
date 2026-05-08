CREATE TABLE IF NOT EXISTS email_domains (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    domain TEXT NOT NULL UNIQUE,
    allow INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
