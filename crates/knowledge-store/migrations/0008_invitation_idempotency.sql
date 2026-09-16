CREATE TABLE invitation_idempotency (
    workspace_id uuid NOT NULL REFERENCES workspaces(id),
    actor_user_id uuid NOT NULL,
    idempotency_key uuid NOT NULL,
    fingerprint bytea NOT NULL CHECK (octet_length(fingerprint) = 32),
    invitation_id uuid NOT NULL REFERENCES invitations(id),
    token_ciphertext bytea NOT NULL,
    token_nonce bytea NOT NULL CHECK (octet_length(token_nonce) = 12),
    token_key_version integer NOT NULL CHECK (token_key_version > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, actor_user_id, idempotency_key),
    FOREIGN KEY (workspace_id, actor_user_id) REFERENCES users(workspace_id, id)
);
