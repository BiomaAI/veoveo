-- Coordinated foundations hard cut. No old audit rows are converted.
REMOVE TABLE IF EXISTS audit_event;
DEFINE TABLE audit_record SCHEMAFULL CHANGEFEED 7d PERMISSIONS NONE;
DEFINE FIELD id ON audit_record TYPE array;
DEFINE FIELD partition ON audit_record TYPE string READONLY;
DEFINE FIELD record_id ON audit_record TYPE uuid READONLY;
DEFINE FIELD class ON audit_record TYPE string READONLY
    ASSERT $value IN ["api_activity", "authentication", "account_change", "artifact_activity", "live_view_access", "computer_activity"];
DEFINE FIELD activity ON audit_record TYPE string READONLY;
DEFINE FIELD outcome ON audit_record TYPE string READONLY
    ASSERT $value IN ["allowed", "denied", "succeeded", "failed"];
DEFINE FIELD actor_key ON audit_record TYPE option<string> READONLY;
DEFINE FIELD target_ref ON audit_record TYPE option<record<artifact_occurrence | computer | task | gateway_task_route | principal | work_context>> READONLY;
DEFINE FIELD trace_id ON audit_record TYPE string READONLY;
DEFINE FIELD request_id ON audit_record TYPE uuid READONLY;
DEFINE FIELD occurred_at ON audit_record TYPE datetime READONLY;
DEFINE FIELD recorded_at ON audit_record TYPE datetime DEFAULT ALWAYS time::now() READONLY;
DEFINE FIELD day ON audit_record TYPE datetime VALUE time::floor(occurred_at, 1d) READONLY;
DEFINE FIELD draft ON audit_record TYPE object FLEXIBLE READONLY;
DEFINE INDEX audit_partition_actor ON audit_record FIELDS partition, actor_key, id;
DEFINE INDEX audit_partition_trace ON audit_record FIELDS partition, trace_id, id;
DEFINE INDEX audit_partition_class ON audit_record FIELDS partition, class, id;
DEFINE INDEX audit_partition_target ON audit_record FIELDS partition, target_ref, id;
DEFINE TABLE audit_daily AS
    SELECT partition, day, class, outcome, count() AS count
    FROM audit_record GROUP BY partition, day, class, outcome;
DEFINE TABLE audit_block SCHEMAFULL CHANGEFEED 7d PERMISSIONS NONE;
DEFINE FIELD partition ON audit_block TYPE string READONLY;
DEFINE FIELD sequence ON audit_block TYPE int READONLY;
DEFINE FIELD first_versionstamp ON audit_block TYPE int READONLY;
DEFINE FIELD last_versionstamp ON audit_block TYPE int READONLY;
DEFINE FIELD records ON audit_block TYPE array<record<audit_record>> READONLY;
DEFINE FIELD block ON audit_block TYPE object FLEXIBLE READONLY;
DEFINE FIELD sealed_at ON audit_block TYPE datetime READONLY;
DEFINE FIELD exported_to ON audit_block TYPE array<string> DEFAULT [];
DEFINE TABLE audit_export_delivery SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD block ON audit_export_delivery TYPE record<audit_block> READONLY;
DEFINE FIELD destination ON audit_export_delivery TYPE string READONLY;
DEFINE FIELD payload ON audit_export_delivery TYPE object FLEXIBLE READONLY;
DEFINE FIELD created_at ON audit_export_delivery TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD delivered_at ON audit_export_delivery TYPE option<datetime>;
DEFINE FIELD rejection ON audit_export_delivery TYPE option<string>;
DEFINE INDEX audit_delivery_block ON audit_export_delivery FIELDS block;
DEFINE TABLE audit_export_cursor SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD sequence ON audit_export_cursor TYPE int;
DEFINE FIELD checkpoint ON audit_export_cursor TYPE object FLEXIBLE;
DEFINE FIELD payload ON audit_export_cursor TYPE object FLEXIBLE;
DEFINE FIELD delivered_at ON audit_export_cursor TYPE datetime;
DEFINE TABLE audit_sealer SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD owner ON audit_sealer TYPE uuid;
DEFINE FIELD generation ON audit_sealer TYPE int;
DEFINE FIELD lease_until ON audit_sealer TYPE datetime;
DEFINE FIELD cursor ON audit_sealer TYPE int;
DEFINE TABLE audit_partition_head SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD partition ON audit_partition_head TYPE string;
DEFINE FIELD sequence ON audit_partition_head TYPE int;
DEFINE FIELD head_hash ON audit_partition_head TYPE string;

DEFINE FIELD replayed_at ON audit_sealer TYPE datetime;
DEFINE FIELD checkpoint ON audit_partition_head TYPE object FLEXIBLE;
DEFINE TABLE audit_retention_anchor SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD partition ON audit_retention_anchor TYPE string;
DEFINE FIELD sequence ON audit_retention_anchor TYPE int;
DEFINE FIELD block ON audit_retention_anchor TYPE object FLEXIBLE;
CREATE audit_sealer:active SET owner = rand::uuid::v7(), generation = 0,
    lease_until = time::now(), cursor = 0, replayed_at = time::now();
DEFINE TABLE audit_record_seal SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD record ON audit_record_seal TYPE record<audit_record> READONLY;
DEFINE FIELD block ON audit_record_seal TYPE record<audit_block> READONLY;

-- Publication keeps the signed completion request through asynchronous finalization.
DEFINE FIELD audit ON artifact_upload TYPE object FLEXIBLE;

-- One first-request receipt per admitted actor, occurrence and five-minute interval.
-- The guard is domain deduplication state; it emits no outbox or changefeed record.
DEFINE TABLE audit_download_window SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD record ON audit_download_window TYPE record<audit_record> READONLY;
DEFINE FIELD window_start ON audit_download_window TYPE datetime READONLY;
DEFINE INDEX audit_download_window_expiry ON audit_download_window FIELDS window_start;
DEFINE FIELD audit ON artifact_read_capability TYPE object FLEXIBLE;
DEFINE FIELD audit ON artifact_write_capability TYPE object FLEXIBLE;

-- All domain callers use checked Rust bindings and share this transaction body.
DEFINE FUNCTION fn::append_audit($audit_rows: array<object>) {
    IF array::len($audit_rows) > 4096 { THROW "audit_batch_limit"; };
    FOR $audit_write IN $audit_rows {
        LET $audit_row = $audit_write.record;
        LET $audit_existing = (SELECT * FROM ONLY $audit_row.id);
        IF $audit_existing != NONE {
            IF $audit_existing.draft != $audit_row.draft { THROW "audit_identity_conflict"; };
        } ELSE {
            LET $window_exists = IF $audit_write.window = NONE { false }
                ELSE { record::exists($audit_write.window) };
            IF !$window_exists {
                CREATE $audit_row.id CONTENT $audit_row;
                IF $audit_write.window != NONE {
                    CREATE ONLY $audit_write.window CONTENT {
                        record: $audit_row.id, window_start: $audit_write.window_start
                    } RETURN NONE;
                };
            };
        };
    };
    RETURN NONE;
} PERMISSIONS NONE;
