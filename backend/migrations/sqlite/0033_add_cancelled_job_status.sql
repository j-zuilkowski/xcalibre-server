-- Phase 28b: Add 'cancelled' to llm_jobs status CHECK constraint
CREATE TABLE llm_jobs_new (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL CHECK(job_type IN ('classify', 'semantic_index', 'quality_check', 'validate_metadata', 'organize', 'derive', 'backup', 'cover_regenerate')),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    book_id TEXT REFERENCES books(id) ON DELETE CASCADE,
    payload_json TEXT,
    result_json TEXT,
    error_text TEXT,
    created_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT
);
INSERT INTO llm_jobs_new SELECT * FROM llm_jobs;
DROP TABLE llm_jobs;
ALTER TABLE llm_jobs_new RENAME TO llm_jobs;
CREATE INDEX IF NOT EXISTS idx_llm_jobs_status ON llm_jobs(status);
CREATE INDEX IF NOT EXISTS idx_llm_jobs_book ON llm_jobs(book_id);
CREATE INDEX IF NOT EXISTS idx_llm_jobs_type ON llm_jobs(job_type);
