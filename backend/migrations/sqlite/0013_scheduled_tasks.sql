CREATE TABLE scheduled_tasks (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    task_type    TEXT NOT NULL CHECK(task_type IN ('classify_all', 'semantic_index_all', 'backup')),
    cron_expr    TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
    last_run_at  TEXT,
    next_run_at  TEXT,
    created_at   TEXT NOT NULL
);

CREATE INDEX idx_scheduled_tasks_due ON scheduled_tasks(enabled, next_run_at);
