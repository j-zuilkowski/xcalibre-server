CREATE VIRTUAL TABLE books_fts USING fts5(
    book_id UNINDEXED,
    title,
    authors,
    tags,
    series,
    content='',
    tokenize='unicode61 remove_diacritics 1'
);

INSERT INTO books_fts (rowid, book_id, title, authors, tags, series)
SELECT
    b.rowid,
    b.id,
    b.title,
    COALESCE((
        SELECT group_concat(author_name, ' ')
        FROM (
            SELECT a.name AS author_name
            FROM book_authors ba
            INNER JOIN authors a ON a.id = ba.author_id
            WHERE ba.book_id = b.id
            ORDER BY ba.display_order ASC, a.sort_name ASC
        )
    ), ''),
    COALESCE((
        SELECT group_concat(tag_name, ' ')
        FROM (
            SELECT t.name AS tag_name
            FROM book_tags bt
            INNER JOIN tags t ON t.id = bt.tag_id
            WHERE bt.book_id = b.id
            ORDER BY t.name ASC
        )
    ), ''),
    COALESCE(s.name, '')
FROM books b
LEFT JOIN series s ON s.id = b.series_id;

CREATE TRIGGER books_fts_books_ai
AFTER INSERT ON books
BEGIN
    INSERT OR REPLACE INTO books_fts (rowid, book_id, title, authors, tags, series)
    SELECT
        b.rowid,
        b.id,
        b.title,
        COALESCE((
            SELECT group_concat(author_name, ' ')
            FROM (
                SELECT a.name AS author_name
                FROM book_authors ba
                INNER JOIN authors a ON a.id = ba.author_id
                WHERE ba.book_id = b.id
                ORDER BY ba.display_order ASC, a.sort_name ASC
            )
        ), ''),
        COALESCE((
            SELECT group_concat(tag_name, ' ')
            FROM (
                SELECT t.name AS tag_name
                FROM book_tags bt
                INNER JOIN tags t ON t.id = bt.tag_id
                WHERE bt.book_id = b.id
                ORDER BY t.name ASC
            )
        ), ''),
        COALESCE(s.name, '')
    FROM books b
    LEFT JOIN series s ON s.id = b.series_id
    WHERE b.rowid = NEW.rowid;
END;

CREATE TRIGGER books_fts_books_au
AFTER UPDATE ON books
BEGIN
    DELETE FROM books_fts WHERE rowid = OLD.rowid;

    INSERT OR REPLACE INTO books_fts (rowid, book_id, title, authors, tags, series)
    SELECT
        b.rowid,
        b.id,
        b.title,
        COALESCE((
            SELECT group_concat(author_name, ' ')
            FROM (
                SELECT a.name AS author_name
                FROM book_authors ba
                INNER JOIN authors a ON a.id = ba.author_id
                WHERE ba.book_id = b.id
                ORDER BY ba.display_order ASC, a.sort_name ASC
            )
        ), ''),
        COALESCE((
            SELECT group_concat(tag_name, ' ')
            FROM (
                SELECT t.name AS tag_name
                FROM book_tags bt
                INNER JOIN tags t ON t.id = bt.tag_id
                WHERE bt.book_id = b.id
                ORDER BY t.name ASC
            )
        ), ''),
        COALESCE(s.name, '')
    FROM books b
    LEFT JOIN series s ON s.id = b.series_id
    WHERE b.rowid = NEW.rowid;
END;

CREATE TRIGGER books_fts_books_ad
AFTER DELETE ON books
BEGIN
    DELETE FROM books_fts WHERE rowid = OLD.rowid;
END;

CREATE TRIGGER books_fts_book_authors_ai
AFTER INSERT ON book_authors
BEGIN
    INSERT OR REPLACE INTO books_fts (rowid, book_id, title, authors, tags, series)
    SELECT
        b.rowid,
        b.id,
        b.title,
        COALESCE((
            SELECT group_concat(author_name, ' ')
            FROM (
                SELECT a.name AS author_name
                FROM book_authors ba
                INNER JOIN authors a ON a.id = ba.author_id
                WHERE ba.book_id = b.id
                ORDER BY ba.display_order ASC, a.sort_name ASC
            )
        ), ''),
        COALESCE((
            SELECT group_concat(tag_name, ' ')
            FROM (
                SELECT t.name AS tag_name
                FROM book_tags bt
                INNER JOIN tags t ON t.id = bt.tag_id
                WHERE bt.book_id = b.id
                ORDER BY t.name ASC
            )
        ), ''),
        COALESCE(s.name, '')
    FROM books b
    LEFT JOIN series s ON s.id = b.series_id
    WHERE b.id = NEW.book_id;
END;

CREATE TRIGGER books_fts_book_authors_au
AFTER UPDATE ON book_authors
BEGIN
    INSERT OR REPLACE INTO books_fts (rowid, book_id, title, authors, tags, series)
    SELECT
        b.rowid,
        b.id,
        b.title,
        COALESCE((
            SELECT group_concat(author_name, ' ')
            FROM (
                SELECT a.name AS author_name
                FROM book_authors ba
                INNER JOIN authors a ON a.id = ba.author_id
                WHERE ba.book_id = b.id
                ORDER BY ba.display_order ASC, a.sort_name ASC
            )
        ), ''),
        COALESCE((
            SELECT group_concat(tag_name, ' ')
            FROM (
                SELECT t.name AS tag_name
                FROM book_tags bt
                INNER JOIN tags t ON t.id = bt.tag_id
                WHERE bt.book_id = b.id
                ORDER BY t.name ASC
            )
        ), ''),
        COALESCE(s.name, '')
    FROM books b
    LEFT JOIN series s ON s.id = b.series_id
    WHERE b.id IN (OLD.book_id, NEW.book_id);
END;

CREATE TRIGGER books_fts_book_authors_ad
AFTER DELETE ON book_authors
BEGIN
    INSERT OR REPLACE INTO books_fts (rowid, book_id, title, authors, tags, series)
    SELECT
        b.rowid,
        b.id,
        b.title,
        COALESCE((
            SELECT group_concat(author_name, ' ')
            FROM (
                SELECT a.name AS author_name
                FROM book_authors ba
                INNER JOIN authors a ON a.id = ba.author_id
                WHERE ba.book_id = b.id
                ORDER BY ba.display_order ASC, a.sort_name ASC
            )
        ), ''),
        COALESCE((
            SELECT group_concat(tag_name, ' ')
            FROM (
                SELECT t.name AS tag_name
                FROM book_tags bt
                INNER JOIN tags t ON t.id = bt.tag_id
                WHERE bt.book_id = b.id
                ORDER BY t.name ASC
            )
        ), ''),
        COALESCE(s.name, '')
    FROM books b
    LEFT JOIN series s ON s.id = b.series_id
    WHERE b.id = OLD.book_id;
END;

CREATE TRIGGER books_fts_book_tags_ai
AFTER INSERT ON book_tags
BEGIN
    INSERT OR REPLACE INTO books_fts (rowid, book_id, title, authors, tags, series)
    SELECT
        b.rowid,
        b.id,
        b.title,
        COALESCE((
            SELECT group_concat(author_name, ' ')
            FROM (
                SELECT a.name AS author_name
                FROM book_authors ba
                INNER JOIN authors a ON a.id = ba.author_id
                WHERE ba.book_id = b.id
                ORDER BY ba.display_order ASC, a.sort_name ASC
            )
        ), ''),
        COALESCE((
            SELECT group_concat(tag_name, ' ')
            FROM (
                SELECT t.name AS tag_name
                FROM book_tags bt
                INNER JOIN tags t ON t.id = bt.tag_id
                WHERE bt.book_id = b.id
                ORDER BY t.name ASC
            )
        ), ''),
        COALESCE(s.name, '')
    FROM books b
    LEFT JOIN series s ON s.id = b.series_id
    WHERE b.id = NEW.book_id;
END;

CREATE TRIGGER books_fts_book_tags_au
AFTER UPDATE ON book_tags
BEGIN
    INSERT OR REPLACE INTO books_fts (rowid, book_id, title, authors, tags, series)
    SELECT
        b.rowid,
        b.id,
        b.title,
        COALESCE((
            SELECT group_concat(author_name, ' ')
            FROM (
                SELECT a.name AS author_name
                FROM book_authors ba
                INNER JOIN authors a ON a.id = ba.author_id
                WHERE ba.book_id = b.id
                ORDER BY ba.display_order ASC, a.sort_name ASC
            )
        ), ''),
        COALESCE((
            SELECT group_concat(tag_name, ' ')
            FROM (
                SELECT t.name AS tag_name
                FROM book_tags bt
                INNER JOIN tags t ON t.id = bt.tag_id
                WHERE bt.book_id = b.id
                ORDER BY t.name ASC
            )
        ), ''),
        COALESCE(s.name, '')
    FROM books b
    LEFT JOIN series s ON s.id = b.series_id
    WHERE b.id IN (OLD.book_id, NEW.book_id);
END;

CREATE TRIGGER books_fts_book_tags_ad
AFTER DELETE ON book_tags
BEGIN
    INSERT OR REPLACE INTO books_fts (rowid, book_id, title, authors, tags, series)
    SELECT
        b.rowid,
        b.id,
        b.title,
        COALESCE((
            SELECT group_concat(author_name, ' ')
            FROM (
                SELECT a.name AS author_name
                FROM book_authors ba
                INNER JOIN authors a ON a.id = ba.author_id
                WHERE ba.book_id = b.id
                ORDER BY ba.display_order ASC, a.sort_name ASC
            )
        ), ''),
        COALESCE((
            SELECT group_concat(tag_name, ' ')
            FROM (
                SELECT t.name AS tag_name
                FROM book_tags bt
                INNER JOIN tags t ON t.id = bt.tag_id
                WHERE bt.book_id = b.id
                ORDER BY t.name ASC
            )
        ), ''),
        COALESCE(s.name, '')
    FROM books b
    LEFT JOIN series s ON s.id = b.series_id
    WHERE b.id = OLD.book_id;
END;

CREATE TRIGGER books_fts_series_au
AFTER UPDATE ON series
BEGIN
    INSERT OR REPLACE INTO books_fts (rowid, book_id, title, authors, tags, series)
    SELECT
        b.rowid,
        b.id,
        b.title,
        COALESCE((
            SELECT group_concat(author_name, ' ')
            FROM (
                SELECT a.name AS author_name
                FROM book_authors ba
                INNER JOIN authors a ON a.id = ba.author_id
                WHERE ba.book_id = b.id
                ORDER BY ba.display_order ASC, a.sort_name ASC
            )
        ), ''),
        COALESCE((
            SELECT group_concat(tag_name, ' ')
            FROM (
                SELECT t.name AS tag_name
                FROM book_tags bt
                INNER JOIN tags t ON t.id = bt.tag_id
                WHERE bt.book_id = b.id
                ORDER BY t.name ASC
            )
        ), ''),
        COALESCE(s.name, '')
    FROM books b
    LEFT JOIN series s ON s.id = b.series_id
    WHERE b.series_id = NEW.id;
END;
