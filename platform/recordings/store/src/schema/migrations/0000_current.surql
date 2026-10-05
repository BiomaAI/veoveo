-- Current owner schema; schema definitions only.

DEFINE TABLE recording TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE recording_blueprint TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE recording_dataset TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE recording_ingest_batch TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE recording_ingest_quota_window TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE recording_ingest_stream TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE recording_layer TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE recording_projection_receipt TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE recording_read_grant TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE FIELD OVERWRITE application_id ON recording TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON recording TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON recording TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON recording TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON recording TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON recording TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON recording TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON recording TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON recording TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON recording TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON recording TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON recording TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON recording TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON recording TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON recording TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON recording TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON recording TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON recording TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE classification ON recording TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON recording TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dataset ON recording TYPE record<recording_dataset> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE delegation_id ON recording TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ended_at ON recording TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE failure_reason ON recording TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON recording TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE initiator ON recording TYPE none | record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE invocation_mode ON recording TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels ON recording TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels.* ON recording TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE last_data_at ON recording TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE manifest_artifact ON recording TYPE none | record<artifact_occurrence> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE metadata ON recording TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON recording TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE policy_revision ON recording TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recording_key ON recording TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON recording TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE seal_task ON recording TYPE none | record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE sealed_at ON recording TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE started_at ON recording TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON recording TYPE 'live' | 'ready' | 'sealing' | 'sealed' | 'interrupted' | 'failed' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON recording TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON recording TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON recording TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX recording_context_started ON recording FIELDS tenant, work_context, started_at;

DEFINE INDEX recording_logical_unique ON recording FIELDS tenant, application_id, recording_key UNIQUE;

DEFINE INDEX recording_owner_started ON recording FIELDS tenant, owner, started_at;

DEFINE INDEX recording_state_started ON recording FIELDS tenant, state, started_at;

DEFINE INDEX recording_tenant_dataset_started ON recording FIELDS tenant, dataset, started_at;

DEFINE FIELD OVERWRITE application_id ON recording_blueprint TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE artifact ON recording_blueprint TYPE none | record<artifact_occurrence> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE blueprint_id ON recording_blueprint TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE byte_len ON recording_blueprint TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON recording_blueprint TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON recording_blueprint TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE message_count ON recording_blueprint TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON recording_blueprint TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE producer_id ON recording_blueprint TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recording ON recording_blueprint TYPE record<recording> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE relative_path ON recording_blueprint TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON recording_blueprint TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE sha256 ON recording_blueprint TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE stream ON recording_blueprint TYPE none | record<recording_ingest_stream> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON recording_blueprint TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON recording_blueprint TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX recording_blueprint_current ON recording_blueprint FIELDS tenant, recording, revision;

DEFINE INDEX recording_blueprint_revision_unique ON recording_blueprint FIELDS tenant, recording, revision UNIQUE;

DEFINE INDEX recording_blueprint_stream ON recording_blueprint FIELDS stream, revision;

DEFINE FIELD OVERWRITE created_at ON recording_dataset TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dataset_key ON recording_dataset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE default_blueprint_artifact ON recording_dataset TYPE none | record<artifact_occurrence> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE display_label ON recording_dataset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON recording_dataset TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE retention_expires_at ON recording_dataset TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE retention_mode ON recording_dataset TYPE 'installation_default' | 'retain_until' | 'retain_forever' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON recording_dataset TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON recording_dataset TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON recording_dataset TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX recording_dataset_tenant_key_unique ON recording_dataset FIELDS tenant, dataset_key UNIQUE;

DEFINE INDEX recording_dataset_tenant_updated ON recording_dataset FIELDS tenant, updated_at;

DEFINE FIELD OVERWRITE byte_len ON recording_ingest_batch TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON recording_ingest_batch TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON recording_ingest_batch TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE materialized_at ON recording_ingest_batch TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE message_count ON recording_ingest_batch TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE payload_format ON recording_ingest_batch TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE producer_id ON recording_ingest_batch TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE relative_path ON recording_ingest_batch TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE `sequence` ON recording_ingest_batch TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE sha256 ON recording_ingest_batch TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON recording_ingest_batch TYPE 'durable' | 'materialized' DEFAULT 'durable' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE stream ON recording_ingest_batch TYPE record<recording_ingest_stream> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON recording_ingest_batch TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX recording_ingest_batch_journal_path_unique ON recording_ingest_batch FIELDS relative_path UNIQUE;

DEFINE INDEX recording_ingest_batch_materialization ON recording_ingest_batch FIELDS stream, state, `sequence`;

DEFINE INDEX recording_ingest_batch_producer_time ON recording_ingest_batch FIELDS tenant, producer_id, created_at;

DEFINE INDEX recording_ingest_batch_sequence_unique ON recording_ingest_batch FIELDS stream, `sequence` UNIQUE;

DEFINE FIELD OVERWRITE batch_count ON recording_ingest_quota_window TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE byte_len ON recording_ingest_quota_window TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON recording_ingest_quota_window TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON recording_ingest_quota_window TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE period ON recording_ingest_quota_window TYPE 'minute' | 'day' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE producer_id ON recording_ingest_quota_window TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON recording_ingest_quota_window TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON recording_ingest_quota_window TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE window_ends_at ON recording_ingest_quota_window TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE window_started_at ON recording_ingest_quota_window TYPE datetime PERMISSIONS FULL;

DEFINE INDEX recording_ingest_quota_window_expiry ON recording_ingest_quota_window FIELDS tenant, producer_id, window_ends_at;

DEFINE INDEX recording_ingest_quota_window_identity_unique ON recording_ingest_quota_window FIELDS tenant, producer_id, period, window_started_at UNIQUE;

DEFINE FIELD OVERWRITE application_id ON recording_ingest_stream TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE byte_len ON recording_ingest_stream TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dataset ON recording_ingest_stream TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE failure_reason ON recording_ingest_stream TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE finished_at ON recording_ingest_stream TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON recording_ingest_stream TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE materialized_through_sequence ON recording_ingest_stream TYPE none | int ASSERT $value = NONE OR $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE message_count ON recording_ingest_stream TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE next_sequence ON recording_ingest_stream TYPE int DEFAULT 1 ASSERT $value >= 1 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE oauth_client_id ON recording_ingest_stream TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE opened_at ON recording_ingest_stream TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON recording_ingest_stream TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE producer_id ON recording_ingest_stream TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recording ON recording_ingest_stream TYPE record<recording> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recording_key ON recording_ingest_stream TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON recording_ingest_stream TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_stream_id ON recording_ingest_stream TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON recording_ingest_stream TYPE 'open' | 'finished' | 'failed' DEFAULT 'open' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON recording_ingest_stream TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON recording_ingest_stream TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX recording_ingest_stream_recording ON recording_ingest_stream FIELDS recording, opened_at;

DEFINE INDEX recording_ingest_stream_source_unique ON recording_ingest_stream FIELDS tenant, producer_id, source_stream_id UNIQUE;

DEFINE INDEX recording_ingest_stream_state ON recording_ingest_stream FIELDS state, updated_at;

DEFINE FIELD OVERWRITE artifact ON recording_layer TYPE none | record<artifact_occurrence> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE byte_len ON recording_layer TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON recording_layer TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE end_time ON recording_layer TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE failure_reason ON recording_layer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON recording_layer TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE kind ON recording_layer TYPE 'capture' | 'properties' | 'derived' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_name ON recording_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE message_count ON recording_layer TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ordinal ON recording_layer TYPE none | int ASSERT $value = NONE OR $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recording ON recording_layer TYPE record<recording> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON recording_layer TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE rrd_version ON recording_layer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE schema_digest ON recording_layer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE sha256 ON recording_layer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE staging_path ON recording_layer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE start_time ON recording_layer TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON recording_layer TYPE 'writing' | 'staged' | 'committed' | 'failed' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON recording_layer TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON recording_layer TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX recording_layer_artifact ON recording_layer FIELDS artifact;

DEFINE INDEX recording_layer_capture_ordinal_unique ON recording_layer FIELDS recording, kind, ordinal UNIQUE;

DEFINE INDEX recording_layer_name_unique ON recording_layer FIELDS recording, layer_name UNIQUE;

DEFINE INDEX recording_layer_staging_path ON recording_layer FIELDS tenant, staging_path;

DEFINE INDEX recording_layer_state_updated ON recording_layer FIELDS tenant, state, updated_at;

DEFINE FIELD OVERWRITE actor ON recording_projection_receipt TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE caller_idempotency_key ON recording_projection_receipt TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE catalog_revision ON recording_projection_receipt TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON recording_projection_receipt TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dataset ON recording_projection_receipt TYPE record<recording_dataset> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON recording_projection_receipt TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE failure_reason ON recording_projection_receipt TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grant ON recording_projection_receipt TYPE record<recording_read_grant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON recording_projection_receipt TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE manifest_digest ON recording_projection_receipt TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE policy_revision ON recording_projection_receipt TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE query_digest ON recording_projection_receipt TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recordings ON recording_projection_receipt TYPE array<record<recording>> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recordings.* ON recording_projection_receipt TYPE record<recording> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE result_byte_len ON recording_projection_receipt TYPE none | int ASSERT $value = NONE OR $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE result_sha256 ON recording_projection_receipt TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON recording_projection_receipt TYPE 'reserved' | 'materializing' | 'ready' | 'failed' | 'cancelled' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON recording_projection_receipt TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON recording_projection_receipt TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON recording_projection_receipt TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX recording_projection_expiry ON recording_projection_receipt FIELDS tenant, expires_at;

DEFINE INDEX recording_projection_idempotency_unique ON recording_projection_receipt FIELDS tenant, actor, caller_idempotency_key UNIQUE;

DEFINE FIELD OVERWRITE actor ON recording_read_grant TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE admitted_set_digest ON recording_read_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE catalog_revision ON recording_read_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON recording_read_grant TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dataset ON recording_read_grant TYPE record<recording_dataset> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON recording_read_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grant_class ON recording_read_grant TYPE 'viewer_segment' | 'catalog_dataset' | 'app_projection' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON recording_read_grant TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE policy_revision ON recording_read_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recordings ON recording_read_grant TYPE array<record<recording>> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recordings.* ON recording_read_grant TYPE record<recording> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON recording_read_grant TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON recording_read_grant TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX recording_read_grant_catalog ON recording_read_grant FIELDS tenant, dataset, admitted_set_digest, grant_class, policy_revision;

DEFINE INDEX recording_read_grant_expiry ON recording_read_grant FIELDS tenant, expires_at;
