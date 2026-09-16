CREATE TABLE operational_settings (
    workspace_id uuid PRIMARY KEY REFERENCES workspaces(id),
    attachment_recovery_days integer NOT NULL DEFAULT 30
        CHECK (attachment_recovery_days BETWEEN 1 AND 3650),
    export_retention_hours integer NOT NULL DEFAULT 24
        CHECK (export_retention_hours BETWEEN 1 AND 720),
    updated_by uuid,
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (workspace_id, updated_by) REFERENCES users(workspace_id, id)
);

INSERT INTO operational_settings (workspace_id)
SELECT id FROM workspaces;
