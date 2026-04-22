CREATE TABLE user_tag_restrictions (
    user_id CHAR(36) NOT NULL,
    tag_id  CHAR(36) NOT NULL,
    mode    VARCHAR(10) NOT NULL,
    PRIMARY KEY (user_id, tag_id),
    CONSTRAINT fk_user_tag_restrictions_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT fk_user_tag_restrictions_tag FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE,
    CONSTRAINT chk_user_tag_restrictions_mode CHECK (mode IN ('allow', 'block'))
);

CREATE INDEX idx_user_tag_restrictions_user ON user_tag_restrictions(user_id);
