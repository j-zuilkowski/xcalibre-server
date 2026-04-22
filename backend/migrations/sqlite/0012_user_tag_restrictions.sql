CREATE TABLE user_tag_restrictions (
    user_id  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tag_id   TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    mode     TEXT NOT NULL CHECK (mode IN ('allow', 'block')),
    PRIMARY KEY (user_id, tag_id)
);

CREATE INDEX idx_user_tag_restrictions_user ON user_tag_restrictions(user_id);
