CREATE TRIGGER IF NOT EXISTS trg_audit_user_role_change
AFTER UPDATE OF role_id ON users
FOR EACH ROW
WHEN OLD.role_id IS NOT NEW.role_id
BEGIN
    INSERT INTO audit_log (id, user_id, action, entity, entity_id, diff_json, created_at)
    VALUES (
        lower(hex(randomblob(16))),
        NEW.id,
        'update',
        'user',
        NEW.id,
        json_object(
            'event', 'role_change',
            'old_role_id', OLD.role_id,
            'new_role_id', NEW.role_id
        ),
        datetime('now')
    );
END;
