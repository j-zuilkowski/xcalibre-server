CREATE VIRTUAL TABLE book_chunks_fts USING fts5(
    text,
    heading_path,
    content='book_chunks',
    content_rowid='rowid'
);

INSERT INTO book_chunks_fts(rowid, text, heading_path)
SELECT rowid, text, heading_path
FROM book_chunks;

CREATE TRIGGER book_chunks_fts_insert AFTER INSERT ON book_chunks BEGIN
    INSERT INTO book_chunks_fts(rowid, text, heading_path)
    VALUES (new.rowid, new.text, new.heading_path);
END;

CREATE TRIGGER book_chunks_fts_delete AFTER DELETE ON book_chunks BEGIN
    INSERT INTO book_chunks_fts(book_chunks_fts, rowid, text, heading_path)
    VALUES ('delete', old.rowid, old.text, old.heading_path);
END;

CREATE TRIGGER book_chunks_fts_update AFTER UPDATE ON book_chunks BEGIN
    INSERT INTO book_chunks_fts(book_chunks_fts, rowid, text, heading_path)
    VALUES ('delete', old.rowid, old.text, old.heading_path);
    INSERT INTO book_chunks_fts(rowid, text, heading_path)
    VALUES (new.rowid, new.text, new.heading_path);
END;
