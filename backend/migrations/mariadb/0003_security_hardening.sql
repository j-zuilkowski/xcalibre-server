CREATE TRIGGER trg_audit_user_role_change
AFTER UPDATE ON users
FOR EACH ROW
INSERT INTO audit_log (id, user_id, action, entity, entity_id, diff_json, created_at)
SELECT
    UUID(),
    NEW.id,
    'update',
    'user',
    NEW.id,
    JSON_OBJECT(
        'event', 'role_change',
        'old_role_id', OLD.role_id,
        'new_role_id', NEW.role_id
    ),
    NOW()
WHERE OLD.role_id <> NEW.role_id;
