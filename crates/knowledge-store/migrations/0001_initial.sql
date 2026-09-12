CREATE TABLE workspaces (
    id uuid PRIMARY KEY,
    name_ciphertext bytea NOT NULL,
    name_nonce bytea NOT NULL CHECK (octet_length(name_nonce) = 12),
    name_key_version integer NOT NULL CHECK (name_key_version > 0),
    wrapped_key_ciphertext bytea NOT NULL,
    wrapped_key_nonce bytea NOT NULL CHECK (octet_length(wrapped_key_nonce) = 12),
    wrapped_key_version integer NOT NULL CHECK (wrapped_key_version > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces(id),
    email_hmac bytea NOT NULL CHECK (octet_length(email_hmac) = 32),
    display_name_ciphertext bytea NOT NULL,
    display_name_nonce bytea NOT NULL CHECK (octet_length(display_name_nonce) = 12),
    display_name_key_version integer NOT NULL CHECK (display_name_key_version > 0),
    password_hash text,
    status text NOT NULL CHECK (status IN ('invited', 'active', 'suspended', 'deleted')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    last_login_at timestamptz,
    deleted_at timestamptz,
    UNIQUE (workspace_id, id)
);

CREATE UNIQUE INDEX users_active_email_hmac_unique
    ON users (workspace_id, email_hmac)
    WHERE deleted_at IS NULL;

CREATE TABLE workspace_members (
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role text NOT NULL CHECK (role IN ('admin', 'member')),
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended')),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id),
    FOREIGN KEY (workspace_id, user_id) REFERENCES users(workspace_id, id)
);

CREATE TABLE sessions (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    token_hash bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    client_metadata_ciphertext bytea,
    client_metadata_nonce bytea CHECK (client_metadata_nonce IS NULL OR octet_length(client_metadata_nonce) = 12),
    client_metadata_key_version integer CHECK (client_metadata_key_version IS NULL OR client_metadata_key_version > 0),
    expires_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (workspace_id, user_id) REFERENCES users(workspace_id, id),
    CHECK (
        (client_metadata_ciphertext IS NULL AND client_metadata_nonce IS NULL AND client_metadata_key_version IS NULL)
        OR
        (client_metadata_ciphertext IS NOT NULL AND client_metadata_nonce IS NOT NULL AND client_metadata_key_version IS NOT NULL)
    )
);

CREATE INDEX sessions_user_active_idx ON sessions (workspace_id, user_id, expires_at)
    WHERE revoked_at IS NULL;

CREATE TABLE invitations (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces(id),
    email_hmac bytea NOT NULL CHECK (octet_length(email_hmac) = 32),
    workspace_role text NOT NULL CHECK (workspace_role IN ('admin', 'member')),
    token_hash bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    invited_by uuid NOT NULL,
    expires_at timestamptz NOT NULL,
    accepted_at timestamptz,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (workspace_id, invited_by) REFERENCES users(workspace_id, id)
);

CREATE INDEX invitations_workspace_email_idx ON invitations (workspace_id, email_hmac);

CREATE TABLE groups (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces(id),
    name_ciphertext bytea NOT NULL,
    name_nonce bytea NOT NULL CHECK (octet_length(name_nonce) = 12),
    name_key_version integer NOT NULL CHECK (name_key_version > 0),
    description_ciphertext bytea,
    description_nonce bytea CHECK (description_nonce IS NULL OR octet_length(description_nonce) = 12),
    description_key_version integer CHECK (description_key_version IS NULL OR description_key_version > 0),
    created_by uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    archived_at timestamptz,
    UNIQUE (workspace_id, id),
    FOREIGN KEY (workspace_id, created_by) REFERENCES users(workspace_id, id),
    CHECK (
        (description_ciphertext IS NULL AND description_nonce IS NULL AND description_key_version IS NULL)
        OR
        (description_ciphertext IS NOT NULL AND description_nonce IS NOT NULL AND description_key_version IS NOT NULL)
    )
);

CREATE TABLE group_members (
    workspace_id uuid NOT NULL,
    group_id uuid NOT NULL,
    user_id uuid NOT NULL,
    added_by uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, user_id),
    FOREIGN KEY (workspace_id, group_id) REFERENCES groups(workspace_id, id),
    FOREIGN KEY (workspace_id, user_id) REFERENCES users(workspace_id, id),
    FOREIGN KEY (workspace_id, added_by) REFERENCES users(workspace_id, id)
);

CREATE INDEX group_members_user_idx ON group_members (workspace_id, user_id);

CREATE TABLE spaces (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces(id),
    name_ciphertext bytea NOT NULL,
    name_nonce bytea NOT NULL CHECK (octet_length(name_nonce) = 12),
    name_key_version integer NOT NULL CHECK (name_key_version > 0),
    description_ciphertext bytea,
    description_nonce bytea CHECK (description_nonce IS NULL OR octet_length(description_nonce) = 12),
    description_key_version integer CHECK (description_key_version IS NULL OR description_key_version > 0),
    created_by uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    archived_at timestamptz,
    UNIQUE (workspace_id, id),
    FOREIGN KEY (workspace_id, created_by) REFERENCES users(workspace_id, id),
    CHECK (
        (description_ciphertext IS NULL AND description_nonce IS NULL AND description_key_version IS NULL)
        OR
        (description_ciphertext IS NOT NULL AND description_nonce IS NOT NULL AND description_key_version IS NOT NULL)
    )
);

CREATE TABLE space_grants (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL,
    space_id uuid NOT NULL,
    user_id uuid,
    group_id uuid,
    role text NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
    granted_by uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (workspace_id, space_id) REFERENCES spaces(workspace_id, id),
    FOREIGN KEY (workspace_id, user_id) REFERENCES users(workspace_id, id),
    FOREIGN KEY (workspace_id, group_id) REFERENCES groups(workspace_id, id),
    FOREIGN KEY (workspace_id, granted_by) REFERENCES users(workspace_id, id),
    CHECK ((user_id IS NOT NULL)::integer + (group_id IS NOT NULL)::integer = 1)
);

CREATE UNIQUE INDEX space_grants_user_unique
    ON space_grants (space_id, user_id) WHERE user_id IS NOT NULL;
CREATE UNIQUE INDEX space_grants_group_unique
    ON space_grants (space_id, group_id) WHERE group_id IS NOT NULL;

CREATE TABLE pages (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL,
    space_id uuid NOT NULL,
    parent_id uuid,
    stable_key uuid NOT NULL,
    sort_key text NOT NULL,
    title_ciphertext bytea NOT NULL,
    title_nonce bytea NOT NULL CHECK (octet_length(title_nonce) = 12),
    title_key_version integer NOT NULL CHECK (title_key_version > 0),
    body_ciphertext bytea NOT NULL,
    body_nonce bytea NOT NULL CHECK (octet_length(body_nonce) = 12),
    body_key_version integer NOT NULL CHECK (body_key_version > 0),
    properties_ciphertext bytea NOT NULL,
    properties_nonce bytea NOT NULL CHECK (octet_length(properties_nonce) = 12),
    properties_key_version integer NOT NULL CHECK (properties_key_version > 0),
    version bigint NOT NULL CHECK (version > 0),
    restricted boolean NOT NULL DEFAULT false,
    created_by uuid NOT NULL,
    updated_by uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    archived_at timestamptz,
    archived_by uuid,
    UNIQUE (workspace_id, id),
    UNIQUE (workspace_id, stable_key),
    UNIQUE (workspace_id, space_id, id),
    FOREIGN KEY (workspace_id, space_id) REFERENCES spaces(workspace_id, id),
    FOREIGN KEY (workspace_id, space_id, parent_id) REFERENCES pages(workspace_id, space_id, id),
    FOREIGN KEY (workspace_id, created_by) REFERENCES users(workspace_id, id),
    FOREIGN KEY (workspace_id, updated_by) REFERENCES users(workspace_id, id),
    FOREIGN KEY (workspace_id, archived_by) REFERENCES users(workspace_id, id)
);

CREATE INDEX pages_tree_idx ON pages (workspace_id, space_id, parent_id, sort_key)
    WHERE archived_at IS NULL;

CREATE TABLE page_revisions (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL,
    page_id uuid NOT NULL,
    version bigint NOT NULL CHECK (version > 0),
    title_ciphertext bytea NOT NULL,
    title_nonce bytea NOT NULL CHECK (octet_length(title_nonce) = 12),
    title_key_version integer NOT NULL CHECK (title_key_version > 0),
    body_ciphertext bytea NOT NULL,
    body_nonce bytea NOT NULL CHECK (octet_length(body_nonce) = 12),
    body_key_version integer NOT NULL CHECK (body_key_version > 0),
    properties_ciphertext bytea NOT NULL,
    properties_nonce bytea NOT NULL CHECK (octet_length(properties_nonce) = 12),
    properties_key_version integer NOT NULL CHECK (properties_key_version > 0),
    content_hash bytea NOT NULL CHECK (octet_length(content_hash) = 32),
    author_id uuid NOT NULL,
    change_reason_ciphertext bytea,
    change_reason_nonce bytea CHECK (change_reason_nonce IS NULL OR octet_length(change_reason_nonce) = 12),
    change_reason_key_version integer CHECK (change_reason_key_version IS NULL OR change_reason_key_version > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (page_id, version),
    FOREIGN KEY (workspace_id, page_id) REFERENCES pages(workspace_id, id),
    FOREIGN KEY (workspace_id, author_id) REFERENCES users(workspace_id, id),
    CHECK (
        (change_reason_ciphertext IS NULL AND change_reason_nonce IS NULL AND change_reason_key_version IS NULL)
        OR
        (change_reason_ciphertext IS NOT NULL AND change_reason_nonce IS NOT NULL AND change_reason_key_version IS NOT NULL)
    )
);

CREATE TABLE page_permissions (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL,
    page_id uuid NOT NULL,
    user_id uuid,
    group_id uuid,
    role text NOT NULL CHECK (role IN ('editor', 'viewer')),
    granted_by uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (workspace_id, page_id) REFERENCES pages(workspace_id, id),
    FOREIGN KEY (workspace_id, user_id) REFERENCES users(workspace_id, id),
    FOREIGN KEY (workspace_id, group_id) REFERENCES groups(workspace_id, id),
    FOREIGN KEY (workspace_id, granted_by) REFERENCES users(workspace_id, id),
    CHECK ((user_id IS NOT NULL)::integer + (group_id IS NOT NULL)::integer = 1)
);

CREATE UNIQUE INDEX page_permissions_user_unique
    ON page_permissions (page_id, user_id) WHERE user_id IS NOT NULL;
CREATE UNIQUE INDEX page_permissions_group_unique
    ON page_permissions (page_id, group_id) WHERE group_id IS NOT NULL;

CREATE TABLE attachments (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL,
    page_id uuid NOT NULL,
    original_name_ciphertext bytea NOT NULL,
    original_name_nonce bytea NOT NULL CHECK (octet_length(original_name_nonce) = 12),
    original_name_key_version integer NOT NULL CHECK (original_name_key_version > 0),
    mime_ciphertext bytea NOT NULL,
    mime_nonce bytea NOT NULL CHECK (octet_length(mime_nonce) = 12),
    mime_key_version integer NOT NULL CHECK (mime_key_version > 0),
    object_key uuid NOT NULL UNIQUE,
    plaintext_size bigint NOT NULL CHECK (plaintext_size >= 0),
    ciphertext_size bigint NOT NULL CHECK (ciphertext_size >= plaintext_size),
    checksum bytea NOT NULL CHECK (octet_length(checksum) = 32),
    key_version integer NOT NULL CHECK (key_version > 0),
    stream_header bytea NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    archived_at timestamptz,
    FOREIGN KEY (workspace_id, page_id) REFERENCES pages(workspace_id, id),
    FOREIGN KEY (workspace_id, created_by) REFERENCES users(workspace_id, id)
);

CREATE INDEX attachments_page_idx ON attachments (workspace_id, page_id)
    WHERE archived_at IS NULL;

CREATE TABLE blind_index_tokens (
    workspace_id uuid NOT NULL,
    page_id uuid NOT NULL,
    token_hmac bytea NOT NULL CHECK (octet_length(token_hmac) = 32),
    field_kind smallint NOT NULL CHECK (field_kind IN (1, 2)),
    prefix_length smallint NOT NULL CHECK (prefix_length BETWEEN 0 AND 12),
    frequency integer NOT NULL CHECK (frequency > 0),
    PRIMARY KEY (page_id, token_hmac, field_kind, prefix_length),
    FOREIGN KEY (workspace_id, page_id) REFERENCES pages(workspace_id, id)
);

CREATE INDEX blind_index_lookup_idx
    ON blind_index_tokens (workspace_id, token_hmac, field_kind, prefix_length);

CREATE TABLE page_links (
    workspace_id uuid NOT NULL,
    source_page_id uuid NOT NULL,
    target_page_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (source_page_id, target_page_id),
    FOREIGN KEY (workspace_id, source_page_id) REFERENCES pages(workspace_id, id),
    FOREIGN KEY (workspace_id, target_page_id) REFERENCES pages(workspace_id, id),
    CHECK (source_page_id <> target_page_id)
);

CREATE INDEX page_links_target_idx ON page_links (workspace_id, target_page_id);

CREATE TABLE audit_events (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces(id),
    actor_user_id uuid,
    session_id uuid,
    event_type text NOT NULL,
    target_type text NOT NULL,
    target_id uuid,
    request_id uuid NOT NULL,
    metadata_ciphertext bytea NOT NULL,
    metadata_nonce bytea NOT NULL CHECK (octet_length(metadata_nonce) = 12),
    metadata_key_version integer NOT NULL CHECK (metadata_key_version > 0),
    previous_hash bytea CHECK (previous_hash IS NULL OR octet_length(previous_hash) = 32),
    event_hash bytea NOT NULL CHECK (octet_length(event_hash) = 32),
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (workspace_id, actor_user_id) REFERENCES users(workspace_id, id),
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE INDEX audit_events_workspace_time_idx
    ON audit_events (workspace_id, created_at DESC, id DESC);

CREATE TABLE jobs (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces(id),
    job_type text NOT NULL,
    payload_ciphertext bytea NOT NULL,
    payload_nonce bytea NOT NULL CHECK (octet_length(payload_nonce) = 12),
    payload_key_version integer NOT NULL CHECK (payload_key_version > 0),
    status text NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    run_after timestamptz NOT NULL DEFAULT now(),
    lease_owner uuid,
    lease_expires_at timestamptz,
    error_summary text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, id),
    CHECK (
        (lease_owner IS NULL AND lease_expires_at IS NULL)
        OR
        (lease_owner IS NOT NULL AND lease_expires_at IS NOT NULL)
    )
);

CREATE INDEX jobs_runnable_idx ON jobs (status, run_after, created_at)
    WHERE status IN ('queued', 'running');

CREATE TABLE exports (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces(id),
    space_id uuid NOT NULL,
    requested_by uuid NOT NULL,
    job_id uuid NOT NULL UNIQUE,
    object_key uuid UNIQUE,
    status text NOT NULL CHECK (status IN ('queued', 'running', 'ready', 'failed', 'expired')),
    expires_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (workspace_id, space_id) REFERENCES spaces(workspace_id, id),
    FOREIGN KEY (workspace_id, requested_by) REFERENCES users(workspace_id, id),
    FOREIGN KEY (workspace_id, job_id) REFERENCES jobs(workspace_id, id)
);

CREATE FUNCTION reject_immutable_row_change() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'rows in % are immutable', TG_TABLE_NAME USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER page_revisions_are_immutable
    BEFORE UPDATE OR DELETE ON page_revisions
    FOR EACH ROW EXECUTE FUNCTION reject_immutable_row_change();
