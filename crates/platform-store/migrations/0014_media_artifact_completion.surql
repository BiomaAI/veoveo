-- Recoverable artifact capability redemptions and media webhook task state.

DEFINE TABLE IF NOT EXISTS artifact_write_redemption SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE artifact_write_redemption TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS capability ON TABLE artifact_write_redemption TYPE record<artifact_write_capability>;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE artifact_write_redemption TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS task ON TABLE artifact_write_redemption TYPE record<task>;
DEFINE FIELD IF NOT EXISTS task_id ON TABLE artifact_write_redemption TYPE string;
DEFINE FIELD IF NOT EXISTS idempotency_key ON TABLE artifact_write_redemption TYPE string;
DEFINE FIELD IF NOT EXISTS request_hash ON TABLE artifact_write_redemption TYPE string ASSERT $value.len() = 64;
DEFINE FIELD IF NOT EXISTS byte_len ON TABLE artifact_write_redemption TYPE int ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS artifact ON TABLE artifact_write_redemption TYPE record<artifact_occurrence>;
DEFINE FIELD IF NOT EXISTS state ON TABLE artifact_write_redemption TYPE "reserved" | "finalized" DEFAULT "reserved";
DEFINE FIELD IF NOT EXISTS reserved_at ON TABLE artifact_write_redemption TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS finalized_at ON TABLE artifact_write_redemption TYPE option<datetime>;
DEFINE INDEX IF NOT EXISTS artifact_write_redemption_capability_key ON TABLE artifact_write_redemption FIELDS capability, idempotency_key UNIQUE;
DEFINE INDEX IF NOT EXISTS artifact_write_redemption_artifact ON TABLE artifact_write_redemption FIELDS artifact UNIQUE;
DEFINE INDEX IF NOT EXISTS artifact_write_redemption_state ON TABLE artifact_write_redemption FIELDS state, reserved_at;

DEFINE TABLE IF NOT EXISTS media_task_context SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE media_task_context TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS task ON TABLE media_task_context TYPE record<task>;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE media_task_context TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS capability ON TABLE media_task_context TYPE record<artifact_write_capability>;
DEFINE FIELD IF NOT EXISTS capability_secret ON TABLE media_task_context TYPE string;
DEFINE FIELD IF NOT EXISTS capability_expires_at ON TABLE media_task_context TYPE datetime;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE media_task_context TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE media_task_context TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS media_task_context_task ON TABLE media_task_context FIELDS task UNIQUE;
DEFINE INDEX IF NOT EXISTS media_task_context_expiry ON TABLE media_task_context FIELDS capability_expires_at;

DEFINE TABLE IF NOT EXISTS media_usage SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE media_usage TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE media_usage TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS task ON TABLE media_usage TYPE record<task>;
DEFINE FIELD IF NOT EXISTS provider_job ON TABLE media_usage TYPE option<record<provider_job>>;
DEFINE FIELD IF NOT EXISTS source_id ON TABLE media_usage TYPE option<string>;
DEFINE FIELD IF NOT EXISTS model_id ON TABLE media_usage TYPE string;
DEFINE FIELD IF NOT EXISTS kind ON TABLE media_usage TYPE "estimate" | "actual";
DEFINE FIELD IF NOT EXISTS quantity ON TABLE media_usage TYPE option<float>;
DEFINE FIELD IF NOT EXISTS unit ON TABLE media_usage TYPE option<string>;
DEFINE FIELD IF NOT EXISTS amount ON TABLE media_usage TYPE option<float>;
DEFINE FIELD IF NOT EXISTS currency ON TABLE media_usage TYPE option<string>;
DEFINE FIELD IF NOT EXISTS metadata ON TABLE media_usage TYPE object FLEXIBLE DEFAULT {};
DEFINE FIELD IF NOT EXISTS recorded_at ON TABLE media_usage TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS media_usage_task_time ON TABLE media_usage FIELDS task, recorded_at;
DEFINE INDEX IF NOT EXISTS media_usage_source_unique ON TABLE media_usage FIELDS task, kind, source_id UNIQUE;
DEFINE INDEX IF NOT EXISTS media_usage_retention ON TABLE media_usage FIELDS recorded_at;
