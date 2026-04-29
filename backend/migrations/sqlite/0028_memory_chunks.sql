CREATE TABLE IF NOT EXISTS memory_chunks (
    id           TEXT    PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    session_id   TEXT,
    project_path TEXT,
    chunk_type   TEXT    NOT NULL DEFAULT 'episodic',
    text         TEXT    NOT NULL,
    tags         TEXT,
    model_id     TEXT    NOT NULL DEFAULT '',
    embedding    BLOB,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_memory_chunks_session_id
    ON memory_chunks(session_id);

CREATE INDEX IF NOT EXISTS idx_memory_chunks_project_path
    ON memory_chunks(project_path);

CREATE INDEX IF NOT EXISTS idx_memory_chunks_created_at
    ON memory_chunks(created_at);

CREATE VIRTUAL TABLE IF NOT EXISTS memory_chunks_fts USING fts5(
    text,
    content = 'memory_chunks',
    content_rowid = 'rowid',
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS memory_chunks_fts_ai
    AFTER INSERT ON memory_chunks BEGIN
        INSERT INTO memory_chunks_fts(rowid, text)
        VALUES (new.rowid, new.text);
    END;

CREATE TRIGGER IF NOT EXISTS memory_chunks_fts_ad
    AFTER DELETE ON memory_chunks BEGIN
        INSERT INTO memory_chunks_fts(memory_chunks_fts, rowid, text)
        VALUES ('delete', old.rowid, old.text);
    END;

CREATE TRIGGER IF NOT EXISTS memory_chunks_fts_au
    AFTER UPDATE ON memory_chunks BEGIN
        INSERT INTO memory_chunks_fts(memory_chunks_fts, rowid, text)
        VALUES ('delete', old.rowid, old.text);
        INSERT INTO memory_chunks_fts(rowid, text)
        VALUES (new.rowid, new.text);
    END;
