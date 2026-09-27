-- Generations invalidate credentials checked before a concurrent revocation,
-- including an active -> suspended -> active transition within one transaction.
ALTER TABLE users ADD COLUMN auth_generation bigint NOT NULL DEFAULT 0;
ALTER TABLE workspace_members ADD COLUMN auth_generation bigint NOT NULL DEFAULT 0;

CREATE FUNCTION bump_auth_generation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.auth_generation := OLD.auth_generation + 1;
    RETURN NEW;
END;
$$;
CREATE TRIGGER users_auth_generation BEFORE UPDATE OF password_hash, status, deleted_at ON users
FOR EACH ROW WHEN (OLD.password_hash IS DISTINCT FROM NEW.password_hash OR OLD.status IS DISTINCT FROM NEW.status OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at)
EXECUTE FUNCTION bump_auth_generation();
CREATE TRIGGER members_auth_generation BEFORE UPDATE OF status, role ON workspace_members
FOR EACH ROW WHEN (OLD.status IS DISTINCT FROM NEW.status OR OLD.role IS DISTINCT FROM NEW.role)
EXECUTE FUNCTION bump_auth_generation();
