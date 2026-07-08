-- Pilot domain: what the agent believes about the world it flies in.
CREATE TABLE IF NOT EXISTS targets (
    target_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    lat DOUBLE NOT NULL,
    lon DOUBLE NOT NULL,
    alt DOUBLE,
    priority INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'active',
    updated_at TIMESTAMP DEFAULT now()
);
CREATE TABLE IF NOT EXISTS missions (
    mission_id TEXT PRIMARY KEY,
    objective TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'open',
    created_at TIMESTAMP DEFAULT now(),
    updated_at TIMESTAMP DEFAULT now()
);
CREATE TABLE IF NOT EXISTS waypoints (
    waypoint_id TEXT PRIMARY KEY,
    mission_id TEXT,
    seq INTEGER NOT NULL,
    lat DOUBLE NOT NULL,
    lon DOUBLE NOT NULL,
    alt DOUBLE,
    eta TIMESTAMP,
    source_task_id TEXT
);
CREATE TABLE IF NOT EXISTS constraints (
    constraint_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    params_json TEXT,
    active BOOLEAN NOT NULL DEFAULT TRUE
);
CREATE TABLE IF NOT EXISTS beliefs (
    belief_id TEXT PRIMARY KEY,
    subject TEXT NOT NULL,
    statement TEXT NOT NULL,
    confidence DOUBLE NOT NULL DEFAULT 0.5,
    source_episode TEXT,
    updated_at TIMESTAMP DEFAULT now()
);
