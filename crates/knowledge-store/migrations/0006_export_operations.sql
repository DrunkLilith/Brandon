ALTER TABLE exports
    ADD COLUMN request_key uuid,
    ADD COLUMN snapshot_ciphertext bytea,
    ADD COLUMN snapshot_nonce bytea CHECK(snapshot_nonce IS NULL OR octet_length(snapshot_nonce)=12),
    ADD COLUMN snapshot_key_version integer CHECK(snapshot_key_version IS NULL OR snapshot_key_version>0),
    ADD COLUMN plaintext_size bigint CHECK(plaintext_size IS NULL OR plaintext_size>=0),
    ADD COLUMN ciphertext_size bigint CHECK(ciphertext_size IS NULL OR ciphertext_size>0),
    ADD COLUMN key_version integer CHECK(key_version IS NULL OR key_version>0),
    ADD COLUMN stream_header bytea,
    ADD COLUMN ready_at timestamptz;

CREATE UNIQUE INDEX exports_request_idempotency_idx
    ON exports(workspace_id,requested_by,request_key) WHERE request_key IS NOT NULL;
ALTER TABLE exports ADD CONSTRAINT exports_workspace_id_id_unique UNIQUE(workspace_id,id);

CREATE TABLE export_staging (
    object_key uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces(id),
    export_id uuid NOT NULL,
    remote_upload_id text,
    expires_at timestamptz NOT NULL DEFAULT now()+interval '1 hour',
    FOREIGN KEY(workspace_id,export_id) REFERENCES exports(workspace_id,id)
);
CREATE INDEX export_staging_expiry_idx ON export_staging(expires_at);
