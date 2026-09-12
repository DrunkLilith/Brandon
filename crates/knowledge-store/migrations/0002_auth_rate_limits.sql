CREATE TABLE login_rate_limits (
    bucket_key bytea PRIMARY KEY CHECK (octet_length(bucket_key) = 32),
    failure_count integer NOT NULL CHECK (failure_count BETWEEN 1 AND 6),
    window_started_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
);

CREATE INDEX login_rate_limits_expiry_idx ON login_rate_limits (updated_at);
