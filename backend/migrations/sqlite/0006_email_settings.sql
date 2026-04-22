CREATE TABLE email_settings (
    id TEXT PRIMARY KEY DEFAULT 'singleton',
    smtp_host TEXT NOT NULL DEFAULT '',
    smtp_port INTEGER NOT NULL DEFAULT 587,
    smtp_user TEXT NOT NULL DEFAULT '',
    smtp_password TEXT NOT NULL DEFAULT '',
    from_address TEXT NOT NULL DEFAULT '',
    use_tls INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL
);
