ALTER TABLE users
    ADD COLUMN email_ciphertext bytea,
    ADD COLUMN email_nonce bytea CHECK (email_nonce IS NULL OR octet_length(email_nonce) = 12),
    ADD COLUMN email_key_version integer CHECK (email_key_version IS NULL OR email_key_version > 0),
    ADD CONSTRAINT users_email_envelope_complete CHECK (
        (email_ciphertext IS NULL AND email_nonce IS NULL AND email_key_version IS NULL)
        OR
        (email_ciphertext IS NOT NULL AND email_nonce IS NOT NULL AND email_key_version IS NOT NULL)
    );

ALTER TABLE invitations
    ADD COLUMN email_ciphertext bytea,
    ADD COLUMN email_nonce bytea CHECK (email_nonce IS NULL OR octet_length(email_nonce) = 12),
    ADD COLUMN email_key_version integer CHECK (email_key_version IS NULL OR email_key_version > 0),
    ADD CONSTRAINT invitations_email_envelope_complete CHECK (
        (email_ciphertext IS NULL AND email_nonce IS NULL AND email_key_version IS NULL)
        OR
        (email_ciphertext IS NOT NULL AND email_nonce IS NOT NULL AND email_key_version IS NOT NULL)
    );

CREATE UNIQUE INDEX invitations_pending_email_unique
    ON invitations (workspace_id, email_hmac)
    WHERE accepted_at IS NULL AND revoked_at IS NULL;
