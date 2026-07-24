-- Hard-cut flat frame records into versioned, immutable world-frame trees.

REMOVE TABLE IF EXISTS frame;

DEFINE TABLE IF NOT EXISTS frame_world SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE frame_world TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE frame_world TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS owner ON TABLE frame_world TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS world_key ON TABLE frame_world TYPE string;
DEFINE FIELD IF NOT EXISTS display_name ON TABLE frame_world TYPE string;
DEFINE FIELD IF NOT EXISTS description ON TABLE frame_world TYPE option<string>;
DEFINE FIELD IF NOT EXISTS head_revision ON TABLE frame_world TYPE option<record<frame_world_revision>>;
DEFINE FIELD IF NOT EXISTS head_revision_key ON TABLE frame_world TYPE option<string>;
DEFINE FIELD IF NOT EXISTS revision ON TABLE frame_world TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS classification ON TABLE frame_world TYPE string;
DEFINE FIELD IF NOT EXISTS labels ON TABLE frame_world TYPE array<string> DEFAULT [];
DEFINE FIELD IF NOT EXISTS created_at ON TABLE frame_world TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE frame_world TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS frame_world_tenant_key_unique ON TABLE frame_world FIELDS tenant, world_key UNIQUE;
DEFINE INDEX IF NOT EXISTS frame_world_owner ON TABLE frame_world FIELDS tenant, owner;

DEFINE TABLE IF NOT EXISTS frame_world_revision SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE frame_world_revision TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE frame_world_revision TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS owner ON TABLE frame_world_revision TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS world ON TABLE frame_world_revision TYPE record<frame_world>;
DEFINE FIELD IF NOT EXISTS world_key ON TABLE frame_world_revision TYPE string;
DEFINE FIELD IF NOT EXISTS revision_key ON TABLE frame_world_revision TYPE string;
DEFINE FIELD IF NOT EXISTS revision ON TABLE frame_world_revision TYPE int ASSERT $value > 0;
DEFINE FIELD IF NOT EXISTS spec_sha256 ON TABLE frame_world_revision TYPE string;
DEFINE FIELD IF NOT EXISTS root_frame_key ON TABLE frame_world_revision TYPE string;
DEFINE FIELD IF NOT EXISTS definition ON TABLE frame_world_revision TYPE object FLEXIBLE;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE frame_world_revision TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS frame_world_revision_key_unique ON TABLE frame_world_revision FIELDS tenant, world_key, revision_key UNIQUE;
DEFINE INDEX IF NOT EXISTS frame_world_revision_number_unique ON TABLE frame_world_revision FIELDS world, revision UNIQUE;
DEFINE INDEX IF NOT EXISTS frame_world_revision_digest ON TABLE frame_world_revision FIELDS world, spec_sha256;
