CREATE TABLE IF NOT EXISTS gateway_control_plane_revisions (
    revision_id TEXT PRIMARY KEY CHECK (revision_id <> ''),
    sha256 TEXT NOT NULL CHECK (sha256 <> ''),
    source TEXT NOT NULL CHECK (
        source IN ('admin_api', 'mounted_file_reload', 'seed_file')
    ),
    applied_at TIMESTAMPTZ NOT NULL,
    applied_by TEXT NOT NULL CHECK (applied_by <> ''),
    tenant TEXT,
    control_plane_json JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_gateway_control_plane_revisions_applied
ON gateway_control_plane_revisions(applied_at DESC, revision_id DESC);

CREATE INDEX IF NOT EXISTS idx_gateway_control_plane_revisions_sha256
ON gateway_control_plane_revisions(sha256);

CREATE TABLE IF NOT EXISTS gateway_control_plane_active (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    revision_id TEXT NOT NULL REFERENCES gateway_control_plane_revisions(revision_id)
);

CREATE TABLE IF NOT EXISTS gateway_control_plane_objects (
    revision_id TEXT NOT NULL REFERENCES gateway_control_plane_revisions(revision_id) ON DELETE CASCADE,
    tenant TEXT,
    object_kind TEXT NOT NULL CHECK (object_kind <> ''),
    object_id TEXT NOT NULL CHECK (object_id <> ''),
    object_json JSONB NOT NULL,
    PRIMARY KEY (revision_id, object_kind, object_id)
);

CREATE INDEX IF NOT EXISTS idx_gateway_control_plane_objects_kind_id
ON gateway_control_plane_objects(object_kind, object_id);

CREATE INDEX IF NOT EXISTS idx_gateway_control_plane_objects_tenant
ON gateway_control_plane_objects(tenant, object_kind);
