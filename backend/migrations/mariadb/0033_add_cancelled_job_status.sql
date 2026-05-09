-- Phase 28b: Add 'cancelled' to llm_jobs status CHECK constraint
ALTER TABLE llm_jobs DROP CONSTRAINT llm_jobs_status_check;
ALTER TABLE llm_jobs DROP CONSTRAINT llm_jobs_job_type_check;
ALTER TABLE llm_jobs ADD CONSTRAINT llm_jobs_status_check CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled'));
ALTER TABLE llm_jobs ADD CONSTRAINT llm_jobs_job_type_check CHECK (job_type IN ('classify', 'semantic_index', 'quality_check', 'validate_metadata', 'organize', 'derive', 'backup', 'cover_regenerate'));
