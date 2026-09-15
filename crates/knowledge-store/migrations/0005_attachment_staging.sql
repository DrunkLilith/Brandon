-- Durable orphan ledger. Session advisory locks protect in-flight objects even
-- after expires_at; a crashed/cancelled writer closes its dedicated connection.
CREATE TABLE attachment_staging (
    object_key uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces(id),
    attachment_id uuid NOT NULL,
    remote_upload_id text,
    expires_at timestamptz NOT NULL DEFAULT now() + interval '1 hour'
);
CREATE INDEX attachment_staging_expiry_idx ON attachment_staging(expires_at);

CREATE TABLE attachment_upload_requests (
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    request_key uuid NOT NULL,
    page_id uuid NOT NULL,
    declared_size bigint NOT NULL CHECK(declared_size > 0),
    object_key uuid NOT NULL UNIQUE,
    attachment_id uuid REFERENCES attachments(id),
    fingerprint bytea CHECK(fingerprint IS NULL OR octet_length(fingerprint) = 32),
    PRIMARY KEY(workspace_id,user_id,request_key),
    FOREIGN KEY(workspace_id,user_id) REFERENCES users(workspace_id,id),
    FOREIGN KEY(workspace_id,page_id) REFERENCES pages(workspace_id,id),
    CHECK((attachment_id IS NULL) = (fingerprint IS NULL))
);
-- Shared domain-separated buckets now support upload limits while auth routes
-- retain the same five-attempt cap and fifteen-minute window.
ALTER TABLE login_rate_limits DROP CONSTRAINT login_rate_limits_failure_count_check;
ALTER TABLE login_rate_limits ADD CONSTRAINT login_rate_limits_failure_count_check
    CHECK (failure_count BETWEEN 1 AND 10001);
