-- Replace filesystem-authoritative recording shards with durable datasets and
-- immutable Artifact-backed Rerun layers. Existing recording data requires an
-- explicit installation reset or a separately approved offline migration.

LET $old_recordings = (SELECT VALUE id FROM recording LIMIT 1);
LET $old_layers = (SELECT VALUE id FROM segment LIMIT 1);
LET $old_streams = (SELECT VALUE id FROM recording_ingest_stream LIMIT 1);
LET $old_batches = (SELECT VALUE id FROM recording_ingest_batch LIMIT 1);
LET $old_blueprints = (SELECT VALUE id FROM recording_blueprint LIMIT 1);
IF array::len($old_recordings) > 0
  OR array::len($old_layers) > 0
  OR array::len($old_streams) > 0
  OR array::len($old_batches) > 0
  OR array::len($old_blueprints) > 0 {
  THROW 'recording_catalog_hard_cut_requires_empty_recording_data';
};

REMOVE TABLE IF EXISTS recording_segment;
REMOVE TABLE IF EXISTS segment;
REMOVE INDEX IF EXISTS recording_tenant_dataset_started ON TABLE recording;
REMOVE FIELD IF EXISTS dataset ON TABLE recording;

DEFINE TABLE IF NOT EXISTS recording_dataset SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE recording_dataset TYPE uuid
  DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE recording_dataset TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS dataset_key ON TABLE recording_dataset TYPE string;
DEFINE FIELD IF NOT EXISTS display_label ON TABLE recording_dataset TYPE string;
DEFINE FIELD IF NOT EXISTS default_blueprint_artifact ON TABLE recording_dataset
  TYPE option<record<artifact_occurrence>>;
DEFINE FIELD IF NOT EXISTS retention_mode ON TABLE recording_dataset
  TYPE "installation_default" | "retain_until" | "retain_forever";
DEFINE FIELD IF NOT EXISTS retention_expires_at ON TABLE recording_dataset
  TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS revision ON TABLE recording_dataset TYPE int
  DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE recording_dataset TYPE datetime
  DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE recording_dataset TYPE datetime
  DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS recording_dataset_tenant_key_unique ON TABLE recording_dataset
  FIELDS tenant, dataset_key UNIQUE;
DEFINE INDEX IF NOT EXISTS recording_dataset_tenant_updated ON TABLE recording_dataset
  FIELDS tenant, updated_at;

DEFINE FIELD IF NOT EXISTS dataset ON TABLE recording TYPE record<recording_dataset>;
DEFINE INDEX IF NOT EXISTS recording_tenant_dataset_started ON TABLE recording
  FIELDS tenant, dataset, started_at;

DEFINE TABLE IF NOT EXISTS recording_layer SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE recording_layer TYPE uuid
  DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE recording_layer TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS recording ON TABLE recording_layer TYPE record<recording>;
DEFINE FIELD IF NOT EXISTS layer_name ON TABLE recording_layer TYPE string;
DEFINE FIELD IF NOT EXISTS kind ON TABLE recording_layer
  TYPE "capture" | "properties" | "derived";
DEFINE FIELD IF NOT EXISTS ordinal ON TABLE recording_layer TYPE option<int>
  ASSERT $value = NONE OR $value >= 0;
DEFINE FIELD IF NOT EXISTS staging_path ON TABLE recording_layer TYPE option<string>;
DEFINE FIELD IF NOT EXISTS artifact ON TABLE recording_layer
  TYPE option<record<artifact_occurrence>>;
DEFINE FIELD IF NOT EXISTS state ON TABLE recording_layer
  TYPE "writing" | "staged" | "committed" | "failed";
DEFINE FIELD IF NOT EXISTS start_time ON TABLE recording_layer TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS end_time ON TABLE recording_layer TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS byte_len ON TABLE recording_layer TYPE int
  DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS message_count ON TABLE recording_layer TYPE int
  DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS sha256 ON TABLE recording_layer TYPE option<string>;
DEFINE FIELD IF NOT EXISTS rrd_version ON TABLE recording_layer TYPE option<string>;
DEFINE FIELD IF NOT EXISTS schema_digest ON TABLE recording_layer TYPE option<string>;
DEFINE FIELD IF NOT EXISTS failure_reason ON TABLE recording_layer TYPE option<string>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE recording_layer TYPE datetime
  DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE recording_layer TYPE datetime
  DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS revision ON TABLE recording_layer TYPE int
  DEFAULT 0 ASSERT $value >= 0;
DEFINE INDEX IF NOT EXISTS recording_layer_name_unique ON TABLE recording_layer
  FIELDS recording, layer_name UNIQUE;
DEFINE INDEX IF NOT EXISTS recording_layer_capture_ordinal_unique ON TABLE recording_layer
  FIELDS recording, kind, ordinal UNIQUE;
DEFINE INDEX IF NOT EXISTS recording_layer_staging_path ON TABLE recording_layer
  FIELDS tenant, staging_path;
DEFINE INDEX IF NOT EXISTS recording_layer_state_updated ON TABLE recording_layer
  FIELDS tenant, state, updated_at;
DEFINE INDEX IF NOT EXISTS recording_layer_artifact ON TABLE recording_layer
  FIELDS artifact;

DEFINE TABLE IF NOT EXISTS recording_read_grant SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE recording_read_grant TYPE uuid
  DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE recording_read_grant TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS dataset ON TABLE recording_read_grant TYPE record<recording_dataset>;
DEFINE FIELD IF NOT EXISTS grant_class ON TABLE recording_read_grant
  TYPE "viewer_segment" | "catalog_dataset" | "app_projection";
DEFINE FIELD IF NOT EXISTS recordings ON TABLE recording_read_grant
  TYPE array<record<recording>>;
DEFINE FIELD IF NOT EXISTS admitted_set_digest ON TABLE recording_read_grant TYPE string;
DEFINE FIELD IF NOT EXISTS actor ON TABLE recording_read_grant TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS work_context ON TABLE recording_read_grant TYPE record<work_context>;
DEFINE FIELD IF NOT EXISTS policy_revision ON TABLE recording_read_grant TYPE string;
DEFINE FIELD IF NOT EXISTS catalog_revision ON TABLE recording_read_grant TYPE string;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE recording_read_grant TYPE datetime;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE recording_read_grant TYPE datetime
  DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS recording_read_grant_expiry ON TABLE recording_read_grant
  FIELDS tenant, expires_at;
DEFINE INDEX IF NOT EXISTS recording_read_grant_catalog ON TABLE recording_read_grant
  FIELDS tenant, dataset, admitted_set_digest, grant_class, policy_revision;

DEFINE TABLE IF NOT EXISTS recording_projection_receipt SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE recording_projection_receipt TYPE uuid
  DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE recording_projection_receipt TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS grant ON TABLE recording_projection_receipt
  TYPE record<recording_read_grant>;
DEFINE FIELD IF NOT EXISTS dataset ON TABLE recording_projection_receipt
  TYPE record<recording_dataset>;
DEFINE FIELD IF NOT EXISTS recordings ON TABLE recording_projection_receipt
  TYPE array<record<recording>>;
DEFINE FIELD IF NOT EXISTS actor ON TABLE recording_projection_receipt
  TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS work_context ON TABLE recording_projection_receipt
  TYPE record<work_context>;
DEFINE FIELD IF NOT EXISTS policy_revision ON TABLE recording_projection_receipt TYPE string;
DEFINE FIELD IF NOT EXISTS catalog_revision ON TABLE recording_projection_receipt TYPE string;
DEFINE FIELD IF NOT EXISTS caller_idempotency_key ON TABLE recording_projection_receipt
  TYPE string;
DEFINE FIELD IF NOT EXISTS manifest_digest ON TABLE recording_projection_receipt TYPE string;
DEFINE FIELD IF NOT EXISTS query_digest ON TABLE recording_projection_receipt TYPE string;
DEFINE FIELD IF NOT EXISTS state ON TABLE recording_projection_receipt
  TYPE "reserved" | "materializing" | "ready" | "failed" | "cancelled";
DEFINE FIELD IF NOT EXISTS result_byte_len ON TABLE recording_projection_receipt
  TYPE option<int> ASSERT $value = NONE OR $value >= 0;
DEFINE FIELD IF NOT EXISTS result_sha256 ON TABLE recording_projection_receipt
  TYPE option<string>;
DEFINE FIELD IF NOT EXISTS failure_reason ON TABLE recording_projection_receipt
  TYPE option<string>;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE recording_projection_receipt TYPE datetime;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE recording_projection_receipt TYPE datetime
  DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE recording_projection_receipt TYPE datetime
  DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS recording_projection_idempotency_unique ON TABLE recording_projection_receipt
  FIELDS tenant, actor, caller_idempotency_key UNIQUE;
DEFINE INDEX IF NOT EXISTS recording_projection_expiry ON TABLE recording_projection_receipt
  FIELDS tenant, expires_at;
