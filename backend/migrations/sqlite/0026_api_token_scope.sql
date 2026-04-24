ALTER TABLE api_tokens ADD COLUMN scope TEXT NOT NULL DEFAULT 'write';
-- Valid values: 'read' | 'write' | 'admin'
