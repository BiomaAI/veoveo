-- Agent-owned orchestration memory. Geographic work remains in Map MCP.
CREATE TABLE IF NOT EXISTS mission_intents (
    mission_key TEXT PRIMARY KEY,
    operator_request TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'received',
    map_route_uri TEXT,
    map_route_digest_sha256 TEXT,
    uav_plan_uri TEXT,
    created_at TIMESTAMP DEFAULT now(),
    updated_at TIMESTAMP DEFAULT now()
);

CREATE TABLE IF NOT EXISTS resource_bookmarks (
    name TEXT PRIMARY KEY,
    uri TEXT NOT NULL,
    revision TEXT,
    digest_sha256 TEXT,
    updated_at TIMESTAMP DEFAULT now()
);
