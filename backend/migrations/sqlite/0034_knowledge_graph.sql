-- Knowledge graph: typed entity-relationship triples written by Merlin sessions
-- or extracted from book content by the ingest pipeline.
--
-- source: 'session' (Merlin agent write) | 'book' (ingest pipeline extraction)
-- source_id: session_id for session triples; book_id for book triples
-- chunk_index: NULL for session triples; the chunk ordinal for book triples

CREATE TABLE IF NOT EXISTS knowledge_graph (
    id          TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    subject     TEXT NOT NULL,
    predicate   TEXT NOT NULL,
    object      TEXT NOT NULL,
    domain_id   TEXT NOT NULL DEFAULT '',
    source      TEXT NOT NULL DEFAULT 'session' CHECK (source IN ('session', 'book')),
    source_id   TEXT NOT NULL DEFAULT '',
    chunk_index INTEGER,
    confidence  REAL NOT NULL DEFAULT 1.0,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_kg_subject   ON knowledge_graph (subject);
CREATE INDEX IF NOT EXISTS idx_kg_object    ON knowledge_graph (object);
CREATE INDEX IF NOT EXISTS idx_kg_domain    ON knowledge_graph (domain_id);
CREATE INDEX IF NOT EXISTS idx_kg_source_id ON knowledge_graph (source_id);

CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_graph_fts
    USING fts5(subject, predicate, object, domain_id,
               content='knowledge_graph', content_rowid='rowid');

CREATE TRIGGER IF NOT EXISTS kg_ai AFTER INSERT ON knowledge_graph BEGIN
    INSERT INTO knowledge_graph_fts(rowid, subject, predicate, object, domain_id)
    VALUES (new.rowid, new.subject, new.predicate, new.object, new.domain_id);
END;

CREATE TRIGGER IF NOT EXISTS kg_ad AFTER DELETE ON knowledge_graph BEGIN
    INSERT INTO knowledge_graph_fts(knowledge_graph_fts, rowid, subject, predicate, object, domain_id)
    VALUES ('delete', old.rowid, old.subject, old.predicate, old.object, old.domain_id);
END;
