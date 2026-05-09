CREATE TABLE IF NOT EXISTS knowledge_graph (
    id          VARCHAR(32) PRIMARY KEY DEFAULT (LOWER(HEX(RANDOM_BYTES(16)))),
    subject     TEXT NOT NULL,
    predicate   TEXT NOT NULL,
    object      TEXT NOT NULL,
    domain_id   TEXT NOT NULL DEFAULT '',
    source      ENUM('session','book') NOT NULL DEFAULT 'session',
    source_id   TEXT NOT NULL DEFAULT '',
    chunk_index INT,
    confidence  DOUBLE NOT NULL DEFAULT 1.0,
    created_at  BIGINT NOT NULL DEFAULT (UNIX_TIMESTAMP())
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX idx_kg_subject   ON knowledge_graph (subject(191));
CREATE INDEX idx_kg_object    ON knowledge_graph (object(191));
CREATE INDEX idx_kg_domain    ON knowledge_graph (domain_id(191));
CREATE INDEX idx_kg_source_id ON knowledge_graph (source_id(191));

CREATE FULLTEXT INDEX idx_kg_fts
    ON knowledge_graph (subject, predicate, object, domain_id);
