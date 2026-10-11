CREATE TABLE retention_purge_marks (
    workspace_id uuid NOT NULL REFERENCES workspaces(id),
    object_type text NOT NULL CHECK (object_type IN ('page', 'group', 'space')),
    object_id uuid NOT NULL,
    archived_at timestamptz NOT NULL,
    started_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (workspace_id, object_type, object_id)
);

CREATE INDEX retention_purge_marks_object_idx
    ON retention_purge_marks (object_type, object_id);

-- Once a bounded purge removes the first immutable revision, later retention
-- setting changes must not block the remaining batches.
CREATE OR REPLACE FUNCTION reject_immutable_row_change() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE fields text[]:=ARRAY['title_ciphertext','title_nonce','title_key_version','body_ciphertext','body_nonce','body_key_version','properties_ciphertext','properties_nonce','properties_key_version','change_reason_ciphertext','change_reason_nonce','change_reason_key_version'];
BEGIN
    IF TG_OP='UPDATE' AND TG_TABLE_NAME='page_revisions'
       AND current_setting('knowledge.rotation_workspace',true)=OLD.workspace_id::text
       AND EXISTS(SELECT 1 FROM key_rotations WHERE workspace_id=OLD.workspace_id AND state='migrating' AND lease_expires_at>clock_timestamp())
       AND (to_jsonb(NEW)-fields)=(to_jsonb(OLD)-fields)
       AND NEW.title_key_version IN (OLD.title_key_version,workspace_current_key_version(OLD.workspace_id))
       AND NEW.body_key_version IN (OLD.body_key_version,workspace_current_key_version(OLD.workspace_id))
       AND NEW.properties_key_version IN (OLD.properties_key_version,workspace_current_key_version(OLD.workspace_id))
       AND (NEW.change_reason_key_version IS NOT DISTINCT FROM OLD.change_reason_key_version OR NEW.change_reason_key_version=workspace_current_key_version(OLD.workspace_id)) THEN RETURN NEW; END IF;
    IF TG_TABLE_NAME = 'page_revisions' AND TG_OP = 'DELETE' AND EXISTS (
        SELECT 1 FROM pages p JOIN spaces s ON s.workspace_id=p.workspace_id AND s.id=p.space_id
        WHERE p.workspace_id=OLD.workspace_id AND p.id=OLD.page_id
          AND (
            retention_expired(p.workspace_id,p.archived_at)
            OR retention_expired(s.workspace_id,s.archived_at)
            OR EXISTS (
                SELECT 1 FROM retention_purge_marks m
                WHERE m.workspace_id=p.workspace_id AND m.object_type='page' AND m.object_id=p.id
            )
          )
    ) THEN RETURN OLD; END IF;
    RAISE EXCEPTION 'immutable record' USING ERRCODE='55000';
END;
$$;
