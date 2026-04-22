CREATE TABLE scheduled_tasks (
    id           CHAR(36) PRIMARY KEY,
    name         VARCHAR(255) NOT NULL,
    task_type    VARCHAR(64) NOT NULL,
    cron_expr    VARCHAR(255) NOT NULL,
    enabled      TINYINT(1) NOT NULL DEFAULT 1,
    last_run_at  DATETIME NULL,
    next_run_at  DATETIME NULL,
    created_at   DATETIME NOT NULL,
    CHECK (task_type IN ('classify_all', 'semantic_index_all', 'backup')),
    CHECK (enabled IN (0, 1))
);

CREATE INDEX idx_scheduled_tasks_due ON scheduled_tasks(enabled, next_run_at);
