ALTER TABLE workspaces ADD COLUMN current_key_version integer NOT NULL DEFAULT 1 CHECK(current_key_version>0);
CREATE TABLE workspace_key_generations (
 workspace_id uuid NOT NULL REFERENCES workspaces(id), version integer NOT NULL CHECK(version>0),
 ciphertext bytea NOT NULL, nonce bytea NOT NULL CHECK(octet_length(nonce)=12), master_version integer NOT NULL CHECK(master_version>0),
 created_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(workspace_id,version)
);
INSERT INTO workspace_key_generations(workspace_id,version,ciphertext,nonce,master_version)
SELECT id,1,wrapped_key_ciphertext,wrapped_key_nonce,wrapped_key_version FROM workspaces;
CREATE FUNCTION capture_workspace_key_generation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO workspace_key_generations(workspace_id,version,ciphertext,nonce,master_version)
 VALUES(NEW.id,NEW.current_key_version,NEW.wrapped_key_ciphertext,NEW.wrapped_key_nonce,NEW.wrapped_key_version)
 ON CONFLICT(workspace_id,version) DO UPDATE SET ciphertext=EXCLUDED.ciphertext,nonce=EXCLUDED.nonce,master_version=EXCLUDED.master_version;
 RETURN NEW;
END $$;
CREATE TRIGGER workspace_key_generation AFTER INSERT OR UPDATE OF wrapped_key_ciphertext,wrapped_key_nonce,wrapped_key_version,current_key_version ON workspaces FOR EACH ROW EXECUTE FUNCTION capture_workspace_key_generation();
CREATE TABLE key_rotations (
 workspace_id uuid PRIMARY KEY REFERENCES workspaces(id), current_version integer NOT NULL, previous_version integer NOT NULL,
 state text NOT NULL CHECK(state IN ('migrating','rebuilding','complete')), after_page uuid, processed bigint NOT NULL DEFAULT 0,
 lease_owner uuid, lease_expires_at timestamptz, lease_generation bigint NOT NULL DEFAULT 0, updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE FUNCTION workspace_current_key_version(workspace uuid) RETURNS integer LANGUAGE sql STABLE AS $$ SELECT current_key_version FROM workspaces WHERE id=workspace $$;
CREATE TABLE key_rotation_objects (
 object_key uuid PRIMARY KEY, workspace_id uuid NOT NULL REFERENCES workspaces(id), entity_id uuid NOT NULL,
 kind text NOT NULL CHECK(kind IN ('attachment','export')), old_object_key uuid NOT NULL, remote_upload_id text,
 published boolean NOT NULL DEFAULT false, expires_at timestamptz NOT NULL DEFAULT now()+interval '10 minutes'
);
-- Immutable revisions may change only their encryption envelopes during the
-- fenced migration. Their IDs, authors, content hashes and all semantic fields
-- remain identical. Audit ciphertext/hash chains are never rewritten.
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
 IF TG_OP='DELETE' AND TG_TABLE_NAME='page_revisions' AND EXISTS(
   SELECT 1 FROM pages p JOIN spaces s ON s.workspace_id=p.workspace_id AND s.id=p.space_id
   WHERE p.workspace_id=OLD.workspace_id AND p.id=OLD.page_id AND (retention_expired(p.workspace_id,p.archived_at) OR retention_expired(s.workspace_id,s.archived_at))
 ) THEN RETURN OLD; END IF;
 RAISE EXCEPTION 'immutable record' USING ERRCODE='55000';
END $$;
