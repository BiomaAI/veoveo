-- Current owner schema and fresh bootstrap state.

DEFINE TABLE audit_block TYPE NORMAL SCHEMAFULL CHANGEFEED 1w PERMISSIONS NONE;

DEFINE TABLE audit_download_window TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE audit_export_cursor TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE audit_export_delivery TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE audit_indexing_receipt TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE audit_indexing_window TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE audit_partition_head TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE audit_record TYPE NORMAL SCHEMAFULL CHANGEFEED 1w PERMISSIONS NONE;

DEFINE TABLE audit_record_seal TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE audit_retention_anchor TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE audit_sealer TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE audit_daily TYPE ANY SCHEMALESS AS SELECT partition, day, class, outcome, count() AS count FROM audit_record GROUP BY partition, day, class, outcome PERMISSIONS NONE;

DEFINE FUNCTION fn::append_audit($audit_rows: array<object>) {
    IF array::len($audit_rows) > 4096 { THROW 'audit_batch_limit'; };
    FOR $audit_write IN $audit_rows {
        LET $audit_row: object = $audit_write.record;
        LET $record_id: record<audit_record> = $audit_row.id;
        LET $audit_existing = SELECT * FROM ONLY $record_id;
        IF type::is_object($audit_existing) THEN {
            IF $audit_existing.draft != $audit_row.draft OR $audit_existing.profile_lookup != $audit_row.profile_lookup OR $audit_existing.target_lookup != $audit_row.target_lookup OR $audit_existing.detail_lookup != $audit_row.detail_lookup { THROW 'audit_identity_conflict'; };
        } ELSE {
            LET $window_exists = IF $audit_write.window = NONE THEN false ELSE {
                LET $window_id: record<audit_download_window> = $audit_write.window;
                RETURN record::exists($window_id);
            } END;
            IF !$window_exists {
                CREATE $record_id CONTENT $audit_row;
                IF $audit_write.window != NONE {
                    LET $window_id: record<audit_download_window> = $audit_write.window;
                    CREATE ONLY $window_id CONTENT { record: $record_id, window_start: $audit_write.window_start } RETURN NONE;
                };
            };
        } END;
    };
    RETURN NONE;
} PERMISSIONS NONE;

DEFINE FUNCTION fn::append_audit_indexing($reads: array<object>) {
    IF array::len($reads) > 64 { THROW 'audit_batch_limit'; };
    LET $now = time::now();
    FOR $read IN $reads {
        LET $receipt_id: record<audit_indexing_receipt> = $read.receipt;
        LET $existing = SELECT * FROM ONLY $receipt_id;
        IF type::is_object($existing) THEN {
            IF $existing.fingerprint != $read.fingerprint { THROW 'audit_identity_conflict'; };
        } ELSE {
            IF $read.occurred_at < $now - 1h OR $read.occurred_at > $now + 1m { THROW 'audit_indexing_read_expired'; };
            LET $start = time::floor($now, 5m);
            LET $id: record<audit_indexing_window> = type::record('audit_indexing_window', [$read.scope, $start]);
            LET $window = SELECT * FROM ONLY $id;
            LET $previous = IF type::is_object($window) THEN $window.members_digest ELSE $read.genesis END;
            LET $digest = IF $read.member = NONE THEN $previous ELSE crypto::sha256('veoveo.ai/audit-indexing-members/v1:' + $previous + $read.member) END;
            IF type::is_object($window) THEN {
                UPDATE ONLY $id SET reads += 1, not_modified += $read.not_modified, failed += $read.failed, members_digest = $digest RETURN NONE;
            } ELSE {
                CREATE ONLY $id CONTENT { start: $start, end: $start + 5m, collection: $read.collection, template: $read.template, reads: 1, not_modified: $read.not_modified, failed: $read.failed, members_digest: $digest } RETURN NONE;
            } END;
            CREATE ONLY $receipt_id CONTENT { fingerprint: $read.fingerprint, recorded_at: $now } RETURN NONE;
        } END;
    };
    RETURN NONE;
} PERMISSIONS NONE;

DEFINE FIELD OVERWRITE block ON audit_block TYPE object FLEXIBLE READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE exported_to ON audit_block TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE exported_to.* ON audit_block TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE first_versionstamp ON audit_block TYPE int READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE last_versionstamp ON audit_block TYPE int READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE partition ON audit_block TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE records ON audit_block TYPE array<record<audit_record>> READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE records.* ON audit_block TYPE record<audit_record> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE sealed_at ON audit_block TYPE datetime READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE `sequence` ON audit_block TYPE int READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record ON audit_download_window TYPE record<audit_record> READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE window_start ON audit_download_window TYPE datetime READONLY PERMISSIONS FULL;

DEFINE INDEX audit_download_window_expiry ON audit_download_window FIELDS window_start;

DEFINE FIELD OVERWRITE checkpoint ON audit_export_cursor TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE delivered_at ON audit_export_cursor TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE payload ON audit_export_cursor TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE `sequence` ON audit_export_cursor TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE block ON audit_export_delivery TYPE record<audit_block> READONLY REFERENCE ON DELETE CASCADE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON audit_export_delivery TYPE datetime DEFAULT time::now() READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE delivered_at ON audit_export_delivery TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE destination ON audit_export_delivery TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE payload ON audit_export_delivery TYPE object FLEXIBLE READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE rejection ON audit_export_delivery TYPE none | string PERMISSIONS FULL;

DEFINE INDEX audit_delivery_block ON audit_export_delivery FIELDS block;

DEFINE FIELD OVERWRITE fingerprint ON audit_indexing_receipt TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recorded_at ON audit_indexing_receipt TYPE datetime READONLY PERMISSIONS FULL;

DEFINE INDEX audit_indexing_receipt_expiry ON audit_indexing_receipt FIELDS recorded_at;

DEFINE FIELD OVERWRITE collection ON audit_indexing_window TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE end ON audit_indexing_window TYPE datetime READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE failed ON audit_indexing_window TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE members_digest ON audit_indexing_window TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE not_modified ON audit_indexing_window TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE reads ON audit_indexing_window TYPE int ASSERT $value > 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE start ON audit_indexing_window TYPE datetime READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE template ON audit_indexing_window TYPE object FLEXIBLE READONLY PERMISSIONS FULL;

DEFINE INDEX audit_indexing_due ON audit_indexing_window FIELDS end;

DEFINE FIELD OVERWRITE checkpoint ON audit_partition_head TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE head_hash ON audit_partition_head TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE partition ON audit_partition_head TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE `sequence` ON audit_partition_head TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE activity ON audit_record TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_key ON audit_record TYPE none | string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE class ON audit_record TYPE string READONLY ASSERT $value INSIDE ['api_activity', 'authentication', 'account_change', 'artifact_activity', 'live_view_access', 'computer_activity'] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE day ON audit_record TYPE datetime READONLY VALUE time::floor(occurred_at, 1d) PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile_lookup ON audit_record TYPE option<string> READONLY PERMISSIONS FULL;
DEFINE FIELD OVERWRITE target_lookup ON audit_record TYPE object FLEXIBLE READONLY PERMISSIONS FULL;
DEFINE FIELD OVERWRITE detail_lookup ON audit_record TYPE object FLEXIBLE READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE draft ON audit_record TYPE object FLEXIBLE READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON audit_record TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE occurred_at ON audit_record TYPE datetime READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE outcome ON audit_record TYPE string READONLY ASSERT $value INSIDE ['allowed', 'denied', 'succeeded', 'failed'] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE partition ON audit_record TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record_id ON audit_record TYPE uuid READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recorded_at ON audit_record TYPE datetime DEFAULT ALWAYS time::now() READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE request_id ON audit_record TYPE uuid READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE target_ref ON audit_record TYPE none | record READONLY ASSERT $value = NONE OR record::tb($value) != '' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE trace_id ON audit_record TYPE string READONLY PERMISSIONS FULL;

DEFINE INDEX audit_partition_actor ON audit_record FIELDS partition, actor_key, id;

DEFINE INDEX audit_partition_class ON audit_record FIELDS partition, class, id;

DEFINE INDEX audit_partition_target ON audit_record FIELDS partition, target_ref, id;

DEFINE INDEX audit_partition_trace ON audit_record FIELDS partition, trace_id, id;

DEFINE FIELD OVERWRITE block ON audit_record_seal TYPE record<audit_block> READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record ON audit_record_seal TYPE record<audit_record> READONLY REFERENCE ON DELETE CASCADE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE block ON audit_retention_anchor TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE partition ON audit_retention_anchor TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE `sequence` ON audit_retention_anchor TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE cursor ON audit_sealer TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE generation ON audit_sealer TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_until ON audit_sealer TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON audit_sealer TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE replayed_at ON audit_sealer TYPE datetime PERMISSIONS FULL;

-- Initial state for a fresh owner lane.
CREATE ONLY audit_sealer:active SET owner = rand::uuid::v7(), generation = 0, lease_until = time::now(), cursor = 0, replayed_at = time::now();
