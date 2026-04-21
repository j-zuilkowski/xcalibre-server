ALTER TABLE books
ADD COLUMN document_type VARCHAR(32) NOT NULL DEFAULT 'unknown' AFTER rating;
