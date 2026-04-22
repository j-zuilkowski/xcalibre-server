CREATE TABLE email_settings (
    id CHAR(36) PRIMARY KEY,
    smtp_host VARCHAR(255) NOT NULL DEFAULT '',
    smtp_port INT NOT NULL DEFAULT 587,
    smtp_user VARCHAR(255) NOT NULL DEFAULT '',
    smtp_password VARCHAR(255) NOT NULL DEFAULT '',
    from_address VARCHAR(255) NOT NULL DEFAULT '',
    use_tls TINYINT(1) NOT NULL DEFAULT 1,
    updated_at DATETIME NOT NULL
);
