CREATE TABLE maintenance_leases (
    workspace_id uuid PRIMARY KEY REFERENCES workspaces(id),
    owner uuid NOT NULL,
    expires_at timestamptz NOT NULL
);

CREATE FUNCTION retention_expired(workspace uuid, archived timestamptz)
RETURNS boolean LANGUAGE sql VOLATILE AS $$
    SELECT archived IS NOT NULL AND archived <= clock_timestamp() - make_interval(days =>
        COALESCE((SELECT attachment_recovery_days FROM operational_settings WHERE workspace_id=workspace), 30))
$$;

-- Revision immutability remains unconditional for UPDATE and live/recoverable
-- pages. Only final retention deletion of an expired parent is permitted.
CREATE OR REPLACE FUNCTION reject_immutable_row_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME = 'page_revisions' AND TG_OP = 'DELETE' AND EXISTS (
        SELECT 1 FROM pages p JOIN spaces s ON s.workspace_id=p.workspace_id AND s.id=p.space_id
        WHERE p.workspace_id=OLD.workspace_id AND p.id=OLD.page_id
          AND (retention_expired(p.workspace_id,p.archived_at) OR retention_expired(s.workspace_id,s.archived_at))
    ) THEN RETURN OLD; END IF;
    RAISE EXCEPTION 'immutable row' USING ERRCODE='55000';
END;
$$;
