CREATE UNIQUE INDEX audit_events_single_root_per_workspace
    ON audit_events (workspace_id)
    WHERE previous_hash IS NULL;

CREATE UNIQUE INDEX audit_events_single_successor_per_hash
    ON audit_events (workspace_id, previous_hash)
    WHERE previous_hash IS NOT NULL;
