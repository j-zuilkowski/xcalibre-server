CREATE INDEX IF NOT EXISTS idx_book_user_state_book_id
    ON book_user_state (book_id);
