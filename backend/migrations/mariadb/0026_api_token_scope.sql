ALTER TABLE api_tokens ADD COLUMN scope VARCHAR(16) NOT NULL DEFAULT 'write';
-- Valid values: 'read' | 'write' | 'admin'
