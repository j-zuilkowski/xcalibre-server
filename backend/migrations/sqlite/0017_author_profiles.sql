CREATE TABLE author_profiles (
    author_id      TEXT PRIMARY KEY REFERENCES authors(id) ON DELETE CASCADE,
    bio            TEXT,
    photo_path     TEXT,
    born           TEXT,
    died           TEXT,
    website_url    TEXT,
    openlibrary_id TEXT,
    updated_at     TEXT NOT NULL
);
